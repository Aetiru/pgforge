import { describe, expect, it } from "vitest";
import { groupRoles, roleGroupOf, roleGroupOpenByDefault, roleRowTags } from "./role-groups";
import type { NodeTag } from "./ipc";

const rol = (label: string, tags: NodeTag[] = []) => ({ label, tags });

describe("roleGroupOf", () => {
  it("manda a Usuarios a quien puede iniciar sesión y a Grupos al resto", () => {
    expect(roleGroupOf(rol("ana", ["login"]))).toBe("users");
    expect(roleGroupOf(rol("ventas", ["group"]))).toBe("groups");
    expect(roleGroupOf(rol("sin_etiquetas"))).toBe("groups");
  });

  it("deja en Sistema a los pg_* aunque no inicien sesión", () => {
    expect(roleGroupOf(rol("pg_monitor", ["group"]))).toBe("system");
  });

  it("un usuario cuyo nombre solo se parece a pg_ no es de sistema", () => {
    expect(roleGroupOf(rol("pgadmin", ["login"]))).toBe("users");
  });
});

describe("groupRoles", () => {
  it("ordena los cajones, conserva el orden interno y omite los vacíos", () => {
    const grupos = groupRoles([
      rol("pg_monitor", ["group"]),
      rol("ana", ["login"]),
      rol("ventas", ["group"]),
      rol("beto", ["login"]),
    ]);
    expect(grupos.map((g) => g.id)).toEqual(["users", "groups", "system"]);
    expect(grupos[0].nodes.map((n) => n.label)).toEqual(["ana", "beto"]);

    expect(groupRoles([rol("ana", ["login"])]).map((g) => g.id)).toEqual(["users"]);
    expect(groupRoles([])).toEqual([]);
  });
});

describe("roleGroupOpenByDefault", () => {
  it("pliega solo Sistema", () => {
    expect(roleGroupOpenByDefault("users")).toBe(true);
    expect(roleGroupOpenByDefault("groups")).toBe(true);
    expect(roleGroupOpenByDefault("system")).toBe(false);
  });
});

describe("roleRowTags", () => {
  it("quita login y grupo y deja las marcas que avisan algo", () => {
    expect(roleRowTags(["login", "superuser", "expired"])).toEqual(["superuser", "expired"]);
    expect(roleRowTags(undefined)).toEqual([]);
  });
});
