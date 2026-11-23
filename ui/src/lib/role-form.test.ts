import { describe, expect, it } from "vitest";

import type { RoleInfo } from "./ipc";
import {
  addDays,
  dangerousAttributes,
  expiryPresetOf,
  generatePassword,
  roleAttributes,
  roleChanges,
  roleForm,
  roleSummary,
  validateRole,
  validUntilDate,
} from "./role-form";

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
    // Un rol nuevo es un usuario: es lo que se crea casi siempre, y un grupo se pide a propósito.
    expect(form.kind).toBe("user");
    expect(form.login).toBe(true);
    // Dar acceso es una decisión: la plantilla arranca sin nada.
    expect(form.access.level).toBe("none");
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

describe("roleForm, tipo de rol existente", () => {
  it("un rol sin login se abre como grupo y uno con login como usuario", () => {
    expect(roleForm({ ...EXISTENTE, login: false }).kind).toBe("group");
    expect(roleForm({ ...EXISTENTE, login: true }).kind).toBe("user");
  });
});

describe("roleChanges, grupo y plantilla de acceso", () => {
  it("un grupo no inicia sesión y no manda contraseña, vencimiento ni límite aunque el formulario los traiga", () => {
    const form = roleForm(null);
    form.name = "g_consulta";
    form.kind = "group";
    form.password = "quedó de antes";
    form.validUntil = "2030-01-01";
    form.connectionLimit = 3;

    const change = roleChanges(form, null, [])[0];
    expect(change.kind).toBe("createRole");
    if (change.kind !== "createRole") return;
    expect(change.attributes.login).toBe(false);
    expect(change.attributes.password).toBeUndefined();
    expect(change.attributes.validUntil).toBeUndefined();
    expect(change.attributes.connectionLimit).toBe(-1);
  });

  it("sin nivel de acceso no agrega ninguna concesión", () => {
    const form = roleForm(null);
    form.name = "ana";
    form.access = { schema: "ventas", level: "none", futureTables: true };
    expect(roleChanges(form, null, [])).toHaveLength(1);
  });

  it("con nivel y esquema agrega la concesión después del alta, con el nombre ya recortado", () => {
    const form = roleForm(null);
    form.name = "  ana  ";
    form.access = { schema: "ventas", level: "write", futureTables: true };
    const changes = roleChanges(form, null, []);
    expect(changes).toHaveLength(2);
    expect(changes[1]).toEqual({
      kind: "grantSchemaAccess",
      role: "ana",
      schema: "ventas",
      level: "write",
      futureTables: true,
    });
  });

  it("un nivel sin esquema elegido no genera nada: no hay sobre qué dar acceso", () => {
    const form = roleForm(null);
    form.name = "ana";
    form.access = { schema: "", level: "read", futureTables: false };
    expect(roleChanges(form, null, [])).toHaveLength(1);
  });

  it("al editar no se ofrece la plantilla: no la considera aunque venga cargada", () => {
    const form = roleForm({ ...EXISTENTE });
    form.access = { schema: "ventas", level: "read", futureTables: false };
    expect(roleChanges(form, { ...EXISTENTE }, [])).toEqual([]);
  });
});

describe("vencimiento", () => {
  const NOCHE = new Date(2026, 10, 23, 23, 30); // 23-nov-2026, 23:30 hora local

  it("cuenta los días con el calendario local y no con UTC", () => {
    // Con `toISOString`, a las 23:30 en una zona detrás de UTC esto daba un día de más.
    expect(addDays(0, NOCHE)).toBe("2026-11-23");
    expect(addDays(30, NOCHE)).toBe("2026-12-23");
  });

  it("pasa de un año a otro y respeta los años bisiestos", () => {
    expect(addDays(365, new Date(2027, 11, 31))).toBe("2028-12-30");
    expect(addDays(1, new Date(2028, 1, 28))).toBe("2028-02-29");
  });

  it("reconoce el atajo que corresponde a una fecha", () => {
    expect(expiryPresetOf("", NOCHE)).toBe("never");
    expect(expiryPresetOf(addDays(90, NOCHE), NOCHE)).toBe("90");
    expect(expiryPresetOf("2031-05-05", NOCHE)).toBe("custom");
  });
});

describe("generatePassword", () => {
  const ALFABETO = /^[abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789_-]+$/;

  it("tiene el largo pedido y solo caracteres sin ambigüedad", () => {
    const pass = generatePassword(24);
    expect(pass).toHaveLength(24);
    expect(pass).toMatch(ALFABETO);
    expect(pass).not.toMatch(/[0OIl1]/);
  });

  it("dos contraseñas seguidas no coinciden", () => {
    expect(generatePassword()).not.toBe(generatePassword());
  });

  it("descarta los bytes del resto del módulo en vez de sesgar hacia el principio del alfabeto", () => {
    // 58 caracteres: 256 - (256 % 58) = 232, así que 232..255 se descartan y no se mapean.
    const bytes = [255, 254, 232, 0, 1, 2];
    let i = 0;
    const pass = generatePassword(3, (size) => {
      const out = new Uint8Array(size);
      for (let k = 0; k < size; k++) out[k] = bytes[i++ % bytes.length];
      return out;
    });
    expect(pass).toBe("abc");
  });

  it("pide más bytes si los primeros se descartaron, hasta completar el largo", () => {
    let calls = 0;
    const pass = generatePassword(5, (size) => {
      calls += 1;
      // La primera tanda se descarta entera; la segunda rinde.
      return new Uint8Array(size).fill(calls === 1 ? 255 : 3);
    });
    expect(pass).toBe("ddddd");
    expect(calls).toBe(2);
  });
});

describe("atributos y resumen", () => {
  it("cada atributo del formulario tiene su explicación", () => {
    const keys = roleAttributes().map((a) => a.key).sort();
    expect(keys).toEqual(["bypassRls", "createdb", "createrole", "inherit", "replication", "superuser"]);
    for (const info of roleAttributes()) expect(info.help.length).toBeGreaterThan(10);
  });

  it("avisa de los de riesgo alto que están tildados y no de `inherit`, que viene tildado de fábrica", () => {
    const form = roleForm(null);
    expect(dangerousAttributes(form)).toEqual([]);
    form.superuser = true;
    form.createdb = true; // riesgo normal: no cuenta
    expect(dangerousAttributes(form).map((a) => a.key)).toEqual(["superuser"]);
  });

  it("el resumen cuenta lo mismo que arma roleChanges", () => {
    const form = roleForm(null);
    form.name = "ana";
    form.memberOf = ["g_consulta"];
    form.validUntil = "2027-01-01";
    form.password = "x";
    form.access = { schema: "ventas", level: "read", futureTables: true };
    const text = roleSummary(form).join("\n");
    expect(text).toContain("usuario ana");
    expect(text).toContain("2027-01-01");
    expect(text).toContain("g_consulta");
    expect(text).toContain("En ventas");
    expect(text).toContain("tablas que se creen después");
  });

  it("el resumen de un grupo no habla de contraseña ni de vencimiento", () => {
    const form = roleForm(null);
    form.name = "g_x";
    form.kind = "group";
    const text = roleSummary(form).join("\n");
    expect(text).toContain("grupo g_x");
    expect(text).not.toContain("contraseña");
    expect(text).not.toContain("vence");
  });
});

describe("roleChanges, ADMIN OPTION en el alta", () => {
  it("con la casilla puesta las membresías salen como GRANT … WITH ADMIN OPTION y no en el CREATE", () => {
    const form = roleForm(null);
    form.name = "ana";
    form.memberOf = ["lectores", "escritores"];
    form.adminOption = true;
    const out = roleChanges(form, null, []);
    expect(out[0]).toMatchObject({ kind: "createRole", memberOf: [] });
    expect(out.slice(1)).toEqual([
      { kind: "grantMembership", role: "lectores", member: "ana", adminOption: true },
      { kind: "grantMembership", role: "escritores", member: "ana", adminOption: true },
    ]);
  });

  it("sin grupos la casilla no genera nada", () => {
    const form = roleForm(null);
    form.name = "ana";
    form.adminOption = true;
    expect(roleChanges(form, null, [])).toHaveLength(1);
  });
});
