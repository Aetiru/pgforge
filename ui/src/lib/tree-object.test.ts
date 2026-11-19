import { describe, expect, it } from "vitest";
import type { TreeNode } from "./ipc";
import { objectRef, parseDragged } from "./tree-object";

const node = (over: Partial<TreeNode>): TreeNode => ({
  id: "x",
  label: "pedidos",
  kind: "table",
  hasChildren: true,
  database: "ventas",
  schema: "public",
  oid: 10,
  ...over,
});

describe("objectRef", () => {
  it("una tabla se escribe calificada", () => {
    expect(objectRef(node({}))).toMatchObject({ family: "relation", qualified: "public.pedidos", hasRows: true });
  });

  it("cita lo que lo necesita", () => {
    expect(objectRef(node({ label: "Mis Pedidos", schema: "user" }))?.qualified).toBe('"user"."Mis Pedidos"');
  });

  it("una función pierde la firma", () => {
    expect(objectRef(node({ kind: "function", label: "calcular_total(p bigint)" }))).toMatchObject({
      family: "routine",
      name: "calcular_total",
      qualified: "public.calcular_total",
      hasRows: false,
    });
  });

  it("lo que no se escribe en una consulta, no es un objeto", () => {
    expect(objectRef(node({ kind: "index" }))).toBeNull();
    expect(objectRef(node({ kind: { folder: "tables" } }))).toBeNull();
    expect(objectRef(node({ schema: undefined }))).toBeNull();
    expect(objectRef(null)).toBeNull();
  });
});

describe("parseDragged", () => {
  it("lee lo que escribió el árbol y descarta lo demás", () => {
    const raw = JSON.stringify({ profileId: "p", database: "d", oid: 3, qualified: "public.t", hasRows: true });
    expect(parseDragged(raw)).toEqual({ profileId: "p", database: "d", oid: 3, qualified: "public.t", hasRows: true });
    expect(parseDragged("no es json")).toBeNull();
    expect(parseDragged(JSON.stringify({ profileId: "p" }))).toBeNull();
    expect(parseDragged(undefined)).toBeNull();
  });
});
