import { describe, expect, it } from "vitest";

import type { RoleInfo } from "./ipc";
import { roleChanges, roleForm, validateRole, validUntilDate } from "./role-form";

const EXISTENTE: RoleInfo = {
  oid: 16384,
  name: "lector",
  superuser: false,
  createdb: false,
  createrole: false,
  inherit: true,
  login: true,
  replication: false,
  bypassRls: false,
  connectionLimit: -1,
  validUntil: null,
};

describe("roleForm", () => {
  it("arranca con los valores por omisión de Postgres cuando el rol es nuevo", () => {
    const form = roleForm(null);
    expect(form.name).toBe("");
    expect(form.inherit).toBe(true);
    expect(form.login).toBe(false);
    // Sin límite es el campo vacío, no un cero: cero conexiones es otra cosa.
    expect(form.connectionLimit).toBeNull();
  });

  it("no precarga la contraseña, porque Postgres no la devuelve", () => {
    expect(roleForm({ ...EXISTENTE }).password).toBe("");
  });

  it("muestra el -1 de «sin límite» como campo vacío y cualquier otro límite tal cual", () => {
    expect(roleForm({ ...EXISTENTE, connectionLimit: -1 }).connectionLimit).toBeNull();
    expect(roleForm({ ...EXISTENTE, connectionLimit: 0 }).connectionLimit).toBe(0);
    expect(roleForm({ ...EXISTENTE, connectionLimit: 5 }).connectionLimit).toBe(5);
  });
});

describe("validUntilDate", () => {
  it("recorta el timestamptz a la fecha que el campo sabe mostrar", () => {
    expect(validUntilDate("2025-12-31 00:00:00+00")).toBe("2025-12-31");
  });

  it("trata «infinity» y la ausencia de fecha como «sin vencimiento»", () => {
    expect(validUntilDate("infinity")).toBe("");
    expect(validUntilDate(null)).toBe("");
    expect(validUntilDate(undefined)).toBe("");
  });
});

describe("roleChanges, rol nuevo", () => {
  it("manda todos los atributos, porque no hay nada contra qué comparar", () => {
    const form = roleForm(null);
    form.name = "  escritor  ";
    form.login = true;
    form.password = "secreta";
    form.memberOf = ["lectores", "escritores"];

    const changes = roleChanges(form, null, []);
    expect(changes).toHaveLength(1);
    expect(changes[0]).toEqual({
      kind: "createRole",
      name: "escritor",
      attributes: {
        superuser: false,
        createdb: false,
        createrole: false,
        inherit: true,
        login: true,
        replication: false,
        bypassRls: false,
        connectionLimit: -1,
        password: "secreta",
        validUntil: undefined,
      },
      memberOf: ["lectores", "escritores"],
    });
  });

  it("omite la contraseña vacía en vez de mandar una cadena vacía", () => {
    const change = roleChanges(roleForm(null), null, [])[0];
    expect(change.kind === "createRole" && change.attributes.password).toBeUndefined();
  });
});

describe("roleChanges, rol existente", () => {
  it("no genera nada cuando el formulario está igual que el servidor", () => {
    expect(roleChanges(roleForm(EXISTENTE), EXISTENTE, [])).toEqual([]);
  });

  it("solo manda en el ALTER los atributos que cambiaron", () => {
    const form = roleForm(EXISTENTE);
    form.createdb = true;

    expect(roleChanges(form, EXISTENTE, [])).toEqual([
      { kind: "alterRole", name: "lector", attributes: { createdb: true } },
    ]);
  });

  it("renombra primero y usa el nombre nuevo en lo que sigue", () => {
    const form = roleForm(EXISTENTE);
    form.name = "lectora";
    form.superuser = true;
    form.memberOf = ["reportes"];

    expect(roleChanges(form, EXISTENTE, [])).toEqual([
      { kind: "renameRole", name: "lector", newName: "lectora" },
      { kind: "alterRole", name: "lectora", attributes: { superuser: true } },
      { kind: "grantMembership", role: "reportes", member: "lectora", adminOption: false },
    ]);
  });

  it("cambia la contraseña solo si se escribió una, y no la considera un atributo más", () => {
    const form = roleForm(EXISTENTE);
    form.password = "nueva";

    expect(roleChanges(form, EXISTENTE, [])).toEqual([
      { kind: "alterRole", name: "lector", attributes: { password: "nueva" } },
    ]);
  });

  it("vaciar el vencimiento lo vuelve a «infinity», que es lo que Postgres entiende", () => {
    const conFecha: RoleInfo = { ...EXISTENTE, validUntil: "2025-12-31 00:00:00+00" };
    const form = roleForm(conFecha);
    form.validUntil = "";

    expect(roleChanges(form, conFecha, [])).toEqual([
      { kind: "alterRole", name: "lector", attributes: { validUntil: "infinity" } },
    ]);
  });

  it("no toca el vencimiento cuando la fecha recortada es la misma que la del servidor", () => {
    const conFecha: RoleInfo = { ...EXISTENTE, validUntil: "2025-12-31 00:00:00+00" };
    expect(roleChanges(roleForm(conFecha), conFecha, [])).toEqual([]);
  });

  it("compara las membresías contra las que había al abrir: otorga las nuevas y revoca las que se sacaron", () => {
    const form = roleForm(EXISTENTE);
    form.memberOf = ["reportes", "auditoria"];
    form.adminOption = true;

    expect(roleChanges(form, EXISTENTE, ["reportes", "backups"])).toEqual([
      { kind: "grantMembership", role: "auditoria", member: "lector", adminOption: true },
      { kind: "revokeMembership", role: "backups", member: "lector" },
    ]);
  });

  it("ignora los nombres vacíos y los repetidos de la lista de membresías", () => {
    const form = roleForm(EXISTENTE);
    form.memberOf = [" reportes ", "", "reportes"];

    expect(roleChanges(form, EXISTENTE, [])).toEqual([
      { kind: "grantMembership", role: "reportes", member: "lector", adminOption: false },
    ]);
  });
});

describe("validateRole", () => {
  it("exige un nombre que no sea solo espacios", () => {
    const form = roleForm(null);
    expect(validateRole(form)).not.toBeNull();
    form.name = "   ";
    expect(validateRole(form)).not.toBeNull();
    form.name = "lector";
    expect(validateRole(form)).toBeNull();
  });

  it("rechaza un límite de conexiones que el servidor no va a aceptar", () => {
    const form = roleForm(null);
    form.name = "lector";
    form.connectionLimit = -3;
    expect(validateRole(form)).not.toBeNull();
    form.connectionLimit = 1.5;
    expect(validateRole(form)).not.toBeNull();
    form.connectionLimit = 0;
    expect(validateRole(form)).toBeNull();
    // Vacío es "sin límite", no un valor inválido.
    form.connectionLimit = null;
    expect(validateRole(form)).toBeNull();
  });
});
