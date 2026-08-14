import { describe, expect, it } from "vitest";
import {
  columnGroupsOf,
  commentTargetOf,
  flagsOf,
  functionSkeleton,
  pathOf,
  privilegeGroupsOf,
  privilegeSubjectOf,
  propertiesOf,
  triggerSummary,
} from "./detail-node";
import type {
  ColumnGrant,
  ConnectionProfile,
  NodeKind,
  PrivilegeGrant,
  ServerCaps,
  TreeNode,
  TriggerInfo,
} from "./ipc";

function node(kind: NodeKind, extra: Partial<TreeNode> = {}): TreeNode {
  return {
    id: "n",
    label: "clientes",
    kind,
    hasChildren: false,
    database: "app",
    ...extra,
  };
}

const PROFILE = {
  id: "p1",
  name: "Local",
  host: "db.interna",
  port: 5432,
  database: "postgres",
  user: "alvaro",
  sslMode: "verifyFull",
  readOnly: false,
  autocommit: true,
} as ConnectionProfile;

describe("flagsOf", () => {
  it("trata a la particionada como tabla, pero no al revés", () => {
    expect(flagsOf(node("partitionedTable")).isTable).toBe(true);
    expect(flagsOf(node("partitionedTable")).isPartitionedTable).toBe(true);
    expect(flagsOf(node("table")).isPartitionedTable).toBe(false);
  });

  it("una función y un procedimiento son los dos rutinas", () => {
    expect(flagsOf(node("function")).isRoutine).toBe(true);
    expect(flagsOf(node("procedure")).isRoutine).toBe(true);
    expect(flagsOf(node("procedure")).isFunction).toBe(false);
  });

  it("una carpeta no tiene DDL propio, y la base tampoco", () => {
    expect(flagsOf(node({ folder: "tables" })).hasDdl).toBe(false);
    expect(flagsOf(node("database")).hasDdl).toBe(false);
    expect(flagsOf(node("table")).hasDdl).toBe(true);
  });

  it("los índices y las restricciones no tienen privilegios: heredan el de la tabla", () => {
    expect(flagsOf(node("index")).hasPrivileges).toBe(false);
    expect(flagsOf(node("constraint")).hasPrivileges).toBe(false);
    expect(flagsOf(node("sequence")).hasPrivileges).toBe(true);
  });

  it("sin nodo no hay ninguna bandera prendida", () => {
    const flags = flagsOf(null);
    expect(Object.values(flags).some(Boolean)).toBe(false);
  });
});

describe("commentTargetOf", () => {
  it("el esquema, la base y el rol se nombran sin esquema", () => {
    expect(commentTargetOf(node("schema", { label: "ventas" }))).toEqual({
      kind: "schema",
      name: "ventas",
    });
    expect(commentTargetOf(node("role", { label: "lector" }))).toEqual({
      kind: "role",
      name: "lector",
    });
  });

  it("una tabla sin esquema cae en public, que es donde el árbol la muestra", () => {
    expect(commentTargetOf(node("table"))).toEqual({
      kind: "table",
      schema: "public",
      name: "clientes",
    });
  });

  it("lo que no se puede comentar devuelve null", () => {
    expect(commentTargetOf(node("function"))).toBeNull();
    expect(commentTargetOf(node({ folder: "tables" }))).toBeNull();
    expect(commentTargetOf(null)).toBeNull();
  });
});

describe("privilegeSubjectOf", () => {
  it("una vista habla el vocabulario de una tabla", () => {
    const view = node("view", { schema: "public", label: "activos" });
    expect(privilegeSubjectOf(view, flagsOf(view), "")).toEqual({
      on: "table",
      schema: "public",
      table: "activos",
    });
  });

  it("una rutina lleva su firma, que es parte de cómo se la nombra en un GRANT", () => {
    const routine = node("procedure", { schema: "app", label: "recalcular" });
    expect(privilegeSubjectOf(routine, flagsOf(routine), "integer, text")).toEqual({
      on: "function",
      schema: "app",
      name: "recalcular",
      args: "integer, text",
      procedure: true,
    });
  });

  it("sin esquema no hay a quién otorgarle nada, salvo la base y el esquema mismos", () => {
    const orphan = node("sequence");
    expect(privilegeSubjectOf(orphan, flagsOf(orphan), "")).toBeNull();
    const database = node("database", { label: "app" });
    expect(privilegeSubjectOf(database, flagsOf(database), "")).toEqual({
      on: "database",
      database: "app",
    });
  });
});

describe("privilegeGroupsOf", () => {
  const grants = [
    { grantee: "lector", privilege: "SELECT", grantable: false },
    { grantee: "lector", privilege: "REFERENCES", grantable: true },
    { grantee: "app", privilege: "INSERT", grantable: false },
  ] as PrivilegeGrant[];

  it("junta una línea por rol y baja los privilegios a minúscula", () => {
    expect(privilegeGroupsOf(grants)).toEqual([
      { grantee: "lector", privileges: ["select", "references"], grantable: true },
      { grantee: "app", privileges: ["insert"], grantable: false },
    ]);
  });

  it("sin privilegios leídos todavía, la lista está vacía", () => {
    expect(privilegeGroupsOf(null)).toEqual([]);
  });
});

describe("columnGroupsOf", () => {
  it("agrupa por la combinación de columna y rol", () => {
    const grants = [
      { column: "salario", grantee: "rrhh", privilege: "SELECT" },
      { column: "salario", grantee: "rrhh", privilege: "UPDATE" },
      { column: "salario", grantee: "app", privilege: "SELECT" },
    ] as ColumnGrant[];
    expect(columnGroupsOf(grants)).toEqual([
      { column: "salario", grantee: "rrhh", privileges: ["SELECT", "UPDATE"] },
      { column: "salario", grantee: "app", privileges: ["SELECT"] },
    ]);
  });

  it("un nombre de columna con el separador adentro no mezcla dos filas", () => {
    const grants = [
      { column: 'a","b', grantee: "x", privilege: "SELECT" },
      { column: "a", grantee: '"b","x', privilege: "UPDATE" },
    ] as ColumnGrant[];
    expect(columnGroupsOf(grants)).toHaveLength(2);
  });
});

describe("triggerSummary", () => {
  it("junta el momento, los eventos y el nivel", () => {
    const trigger = {
      timing: "before",
      events: ["insert", "update"],
      level: "row",
    } as TriggerInfo;
    expect(triggerSummary(trigger)).toBe("BEFORE INSERT OR UPDATE · ROW");
  });
});

describe("functionSkeleton", () => {
  it("un procedimiento no declara qué devuelve", () => {
    expect(functionSkeleton("app", true)).toContain("CREATE PROCEDURE app.nombre()");
    expect(functionSkeleton("app", true)).not.toContain("RETURNS");
    expect(functionSkeleton("app", false)).toContain("RETURNS void");
  });
});

describe("propertiesOf", () => {
  it("marca en ámbar la producción y los permisos que faltan", () => {
    const caps = {
      version: 160004,
      isSuperuser: false,
      canSignalBackends: false,
      canReadAllStats: true,
    } as ServerCaps;
    const rows = propertiesOf(true, null, { ...PROFILE, environment: "prod" }, caps);
    expect(rows.find((row) => row.label === "Entorno")).toMatchObject({ bad: true });
    expect(rows.find((row) => row.label === "Puede cancelar sesiones")).toMatchObject({ bad: true });
    expect(rows.find((row) => row.label === "Ve todas las estadísticas")).toMatchObject({
      bad: false,
    });
  });

  it("un objeto muestra su base, su esquema y su OID, y omite lo que no tiene", () => {
    expect(propertiesOf(false, node("table", { schema: "public", oid: 42 }), null, null)).toEqual([
      { label: "Base de datos", value: "app" },
      { label: "Esquema", value: "public" },
      { label: "OID", value: "42" },
    ]);
    expect(propertiesOf(false, node("database"), null, null)).toEqual([
      { label: "Base de datos", value: "app" },
    ]);
  });
});

describe("pathOf", () => {
  it("un servidor muestra dónde vive y un objeto de dónde salió", () => {
    expect(pathOf(true, null, PROFILE)).toBe("db.interna:5432");
    expect(pathOf(false, node("table", { schema: "public" }), PROFILE)).toBe("app / public");
    expect(pathOf(false, node("database"), PROFILE)).toBe("app");
  });
});
