import { describe, expect, it } from "vitest";
import { dataTargetOf, erdTargetOf, qualifiedNameOf, queryTargetOf } from "./tree-actions";
import type { Row } from "./explorer.svelte";
import type { ConnectionProfile, NodeKind, TreeNode } from "./ipc";

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

function row(extra: Partial<Row> = {}): Row {
  return {
    kind: "node",
    key: "k",
    profileId: "p1",
    node: node("table", { oid: 42, schema: "public" }),
    level: 2,
    label: "clientes",
    hasChildren: false,
    expanded: false,
    loading: false,
    children: null,
    connected: false,
    ...extra,
  };
}

const PROFILE = { id: "p1", name: "Local", database: "postgres" } as ConnectionProfile;

describe("queryTargetOf", () => {
  it("usa la base que el objeto trae encima", () => {
    expect(queryTargetOf(row(), PROFILE)).toEqual({ database: "app", title: "clientes" });
  });

  it("en la fila del servidor conectado usa la base del perfil", () => {
    const server = row({ kind: "server", node: null, connected: true, label: "Local" });
    expect(queryTargetOf(server, PROFILE)).toEqual({ database: "postgres", title: "Local" });
  });

  it("no hay contra qué consultar en un servidor apagado", () => {
    const server = row({ kind: "server", node: null, connected: false });
    expect(queryTargetOf(server, PROFILE)).toBeNull();
  });

  it("una carpeta de conexiones no apunta a ninguna base", () => {
    const group = row({ kind: "group", node: null, profileId: "", connected: false });
    expect(queryTargetOf(group, undefined)).toBeNull();
  });

  it("sin fila seleccionada no hay destino", () => {
    expect(queryTargetOf(null, PROFILE)).toBeNull();
  });
});

describe("dataTargetOf", () => {
  it("devuelve el oid de las relaciones que tienen filas", () => {
    for (const kind of [
      "table",
      "partitionedTable",
      "view",
      "materializedView",
      "foreignTable",
    ] as NodeKind[]) {
      expect(dataTargetOf(node(kind, { oid: 7 }))).toBe(7);
    }
  });

  it("no abre datos de lo que no es una relación", () => {
    expect(dataTargetOf(node("index", { oid: 7 }))).toBeNull();
    expect(dataTargetOf(node("schema", { oid: 7 }))).toBeNull();
  });

  it("sin oid no hay nada que consultar, aunque el tipo sirva", () => {
    expect(dataTargetOf(node("table"))).toBeNull();
    expect(dataTargetOf(null)).toBeNull();
  });
});

describe("erdTargetOf", () => {
  it("solo un esquema tiene diagrama", () => {
    expect(erdTargetOf(node("schema", { label: "public" }))).toEqual({
      database: "app",
      schema: "public",
    });
    expect(erdTargetOf(node("table"))).toBeNull();
    expect(erdTargetOf(null)).toBeNull();
  });
});

describe("qualifiedNameOf", () => {
  it("califica con el esquema cuando el objeto vive en uno", () => {
    expect(qualifiedNameOf(node("table", { schema: "ventas" }))).toBe("ventas.clientes");
    expect(qualifiedNameOf(node("database", { label: "app" }))).toBe("app");
    expect(qualifiedNameOf(null)).toBeNull();
  });
});
