import { describe, expect, it } from "vitest";

import type { PolicyInfo } from "./ipc";
import {
  acceptsCheck,
  acceptsUsing,
  policyChanges,
  policyForm,
  validatePolicy,
} from "./policy-form";

const TARGET = { schema: "public", table: "clientes" };

const EXISTENTE: PolicyInfo = {
  oid: 16400,
  name: "solo_propias",
  command: "select",
  kind: "permissive",
  roles: ["ana", "beto"],
  using: "dueno = current_user",
  check: null,
};

describe("acceptsUsing y acceptsCheck", () => {
  it("un INSERT no filtra filas previas y un SELECT o DELETE no escribe ninguna", () => {
    expect(acceptsUsing("insert")).toBe(false);
    expect(acceptsUsing("select")).toBe(true);
    expect(acceptsCheck("select")).toBe(false);
    expect(acceptsCheck("delete")).toBe(false);
    expect(acceptsCheck("insert")).toBe(true);
    expect(acceptsCheck("update")).toBe(true);
    expect(acceptsUsing("all")).toBe(true);
    expect(acceptsCheck("all")).toBe(true);
  });
});

describe("policyForm", () => {
  it("arranca en ALL permisiva y sin roles, que es lo más amplio y lo más común", () => {
    const form = policyForm(null);
    expect(form.command).toBe("all");
    expect(form.kind).toBe("permissive");
    expect(form.roles).toBe("");
  });

  it("carga la política existente con los roles como texto editable", () => {
    expect(policyForm(EXISTENTE).roles).toBe("ana, beto");
  });
});

describe("policyChanges", () => {
  it("da de alta con una sola sentencia cuando no hay política previa", () => {
    const form = policyForm(null);
    form.name = " solo_propias ";
    form.command = "select";
    form.roles = "ana, beto";
    form.using = " dueno = current_user ";

    expect(policyChanges(form, TARGET, null)).toEqual([
      {
        kind: "createPolicy",
        schema: "public",
        table: "clientes",
        name: "solo_propias",
        definition: {
          command: "select",
          kind: "permissive",
          roles: ["ana", "beto"],
          using: "dueno = current_user",
          check: null,
        },
      },
    ]);
  });

  it("editar es borrar y crear de nuevo, y el DROP va con el nombre viejo", () => {
    const form = policyForm(EXISTENTE);
    form.name = "solo_mias";

    const changes = policyChanges(form, TARGET, EXISTENTE);
    expect(changes).toHaveLength(2);
    expect(changes[0]).toEqual({
      kind: "dropPolicy",
      schema: "public",
      table: "clientes",
      name: "solo_propias",
    });
    expect(changes[1].kind).toBe("createPolicy");
  });

  it("descarta la expresión que el comando no acepta aunque haya quedado escrita", () => {
    const form = policyForm(null);
    form.name = "alta";
    form.command = "insert";
    form.using = "dueno = current_user";
    form.check = "dueno = current_user";

    const change = policyChanges(form, TARGET, null)[0];
    expect(change.kind === "createPolicy" && change.definition.using).toBeNull();
    expect(change.kind === "createPolicy" && change.definition.check).toBe("dueno = current_user");
  });

  it("una expresión en blanco es la ausencia de expresión, no una condición vacía", () => {
    const form = policyForm(null);
    form.name = "todo";
    form.using = "   ";

    const change = policyChanges(form, TARGET, null)[0];
    expect(change.kind === "createPolicy" && change.definition.using).toBeNull();
  });

  it("sin roles la política queda en PUBLIC, que es la lista vacía", () => {
    const form = policyForm(null);
    form.name = "todo";
    form.roles = " , ";

    const change = policyChanges(form, TARGET, null)[0];
    expect(change.kind === "createPolicy" && change.definition.roles).toEqual([]);
  });
});

describe("validatePolicy", () => {
  it("exige un nombre que no sea solo espacios", () => {
    const form = policyForm(null);
    expect(validatePolicy(form)).not.toBeNull();
    form.name = "  ";
    expect(validatePolicy(form)).not.toBeNull();
    form.name = "solo_propias";
    expect(validatePolicy(form)).toBeNull();
  });
});
