import { describe, expect, it } from "vitest";
import type { TreeNode } from "./ipc";
import { groupByPrefix, orderNodes, prefixOf } from "./tree-order";

const node = (label: string, extra: Partial<TreeNode> = {}): TreeNode => ({
  id: label,
  label,
  kind: "table",
  hasChildren: true,
  database: "db",
  ...extra,
});

describe("orderNodes", () => {
  const list = [node("b", { sizeBytes: 10, rows: 5 }), node("a", { sizeBytes: 900, rows: 1 }), node("z"), node("c", { sizeBytes: 10 })];

  it("por nombre, sin distinguir mayúsculas", () => {
    expect(orderNodes([node("B"), node("a")], "name").map((n) => n.label)).toEqual(["a", "B"]);
  });

  it("por tamaño va de mayor a menor y lo que no tiene dato, al final", () => {
    expect(orderNodes(list, "size").map((n) => n.label)).toEqual(["a", "b", "c", "z"]);
  });

  it("por filas, igual", () => {
    expect(orderNodes(list, "rows").map((n) => n.label)).toEqual(["b", "a", "c", "z"]);
  });

  it("no modifica la lista que recibe", () => {
    const original = [node("b"), node("a")];
    orderNodes(original, "name");
    expect(original.map((n) => n.label)).toEqual(["b", "a"]);
  });
});

describe("prefixOf", () => {
  it("toma hasta el primer guion bajo, que no puede ser el primero ni el último carácter", () => {
    expect(prefixOf("fact_ventas")).toBe("fact_");
    expect(prefixOf("_tmp")).toBeNull();
    expect(prefixOf("tmp_")).toBeNull();
    expect(prefixOf("ventas")).toBeNull();
  });
});

describe("groupByPrefix", () => {
  const names = ["dim_a", "dim_b", "dim_c", "fact_x", "fact_y", "fact_z", "fact_w", "clientes", "tmp_1"];
  const nodes = names.map((n) => node(n));

  it("agrupa los prefijos con tres o más y deja el resto suelto", () => {
    const result = groupByPrefix(nodes)!;
    expect(result.groups.map((g) => [g.prefix, g.nodes.length])).toEqual([["dim_", 3], ["fact_", 4]]);
    expect(result.loose.map((n) => n.label)).toEqual(["clientes", "tmp_1"]);
  });

  it("sin ningún grupo, o con un solo grupo que es todo, devuelve null", () => {
    expect(groupByPrefix([node("a"), node("b")])).toBeNull();
    expect(groupByPrefix([node("x_1"), node("x_2"), node("x_3")])).toBeNull();
  });
});
