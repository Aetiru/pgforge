import { describe, expect, it } from "vitest";
import {
  connectionUrl,
  dataTargetOf,
  erdTargetOf,
  folderForKind,
  qualifiedNameOf,
  queryTargetOf,
} from "./tree-actions";
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

describe("folderForKind", () => {
  it("manda cada tipo al cajón donde el árbol lo muestra", () => {
    expect(folderForKind("table")).toBe("tables");
    expect(folderForKind("partitionedTable")).toBe("tables");
    expect(folderForKind("materializedView")).toBe("materializedViews");
    expect(folderForKind("sequence")).toBe("sequences");
    expect(folderForKind("procedure")).toBe("procedures");
    expect(folderForKind("type")).toBe("types");
  });

  it("no hay cajón para lo que no cuelga de un esquema", () => {
    expect(folderForKind("database")).toBeNull();
    expect(folderForKind("role")).toBeNull();
    expect(folderForKind({ folder: "tables" })).toBeNull();
  });
});

describe("connectionUrl", () => {
  const profile = (extra: Partial<ConnectionProfile>) =>
    ({ ...PROFILE, host: "db.local", port: 5432, user: "app", ...extra }) as ConnectionProfile;

  it("arma la URL con lo que identifica al servidor", () => {
    expect(connectionUrl(profile({}))).toBe("postgres://app@db.local:5432/postgres");
  });

  it("escapa el usuario y la base: una arroba adentro partiría la URL", () => {
    expect(connectionUrl(profile({ user: "admin@casa", database: "mi base" }))).toBe(
      "postgres://admin%40casa@db.local:5432/mi%20base",
    );
  });
});

describe("qualifiedNameOf", () => {
  it("califica con el esquema cuando el objeto vive en uno", () => {
    expect(qualifiedNameOf(node("table", { schema: "ventas" }))).toBe("ventas.clientes");
    expect(qualifiedNameOf(node("database", { label: "app" }))).toBe("app");
    expect(qualifiedNameOf(null)).toBeNull();
  });
});
