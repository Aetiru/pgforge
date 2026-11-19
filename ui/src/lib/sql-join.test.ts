import { describe, expect, it } from "vitest";
import type { ForeignKeyRef } from "./ipc";
import { ident, joinConditions, joinContext } from "./sql-join";
import type { TableRef } from "./sql-complete";

const fk = (table: string, columns: string[], refTable: string, refColumns: string[]): ForeignKeyRef => ({
  schema: "public",
  table,
  columns,
  refSchema: "public",
  refTable,
  refColumns,
});
const ref = (name: string, alias: string | null = null): TableRef => ({ schema: null, name, alias });
const publicSchema = () => "public";

describe("contexto de un JOIN", () => {
  it("reconoce el lugar del ON, con y sin alias", () => {
    expect(joinContext("SELECT * FROM pedidos p JOIN clientes c ")).toMatchObject({
      joined: { name: "clientes", alias: "c" },
      hasOn: false,
      partial: "",
    });
    expect(joinContext("... JOIN clientes ")).toMatchObject({ joined: { name: "clientes", alias: null } });
    expect(joinContext("... JOIN clientes c o")).toMatchObject({ partial: "o", hasOn: false });
  });

  it("con ON escrito ofrece solo la condición", () => {
    expect(joinContext("... JOIN clientes c ON ")).toMatchObject({ hasOn: true, partial: "" });
    expect(joinContext("... JOIN clientes ON ")).toMatchObject({
      joined: { name: "clientes", alias: null },
      hasOn: true,
    });
  });

  it("no ofrece nada fuera de un JOIN", () => {
    expect(joinContext("SELECT * FROM pedidos ")).toBeNull();
    expect(joinContext("... JOIN clientes c WHERE ")).toBeNull();
  });

  it("toma esquema.tabla", () => {
    expect(joinContext("... JOIN ventas.clientes c ")?.joined).toMatchObject({ schema: "ventas", name: "clientes" });
  });
});

describe("condiciones desde claves foráneas", () => {
  const keys = [fk("pedidos", ["cliente_id"], "clientes", ["id"])];

  it("la nueva es la referida", () => {
    const out = joinConditions(ref("clientes", "c"), [ref("pedidos", "p")], keys, publicSchema);
    expect(out.map((o) => o.label)).toEqual(["p.cliente_id = c.id"]);
  });

  it("la nueva es la que referencia", () => {
    const out = joinConditions(ref("pedidos", "p"), [ref("clientes", "c")], keys, publicSchema);
    expect(out.map((o) => o.label)).toEqual(["p.cliente_id = c.id"]);
  });

  it("una clave compuesta se une con AND, en orden", () => {
    const compuesta = [fk("lineas", ["a", "b"], "cab", ["x", "y"])];
    const out = joinConditions(ref("cab"), [ref("lineas")], compuesta, publicSchema);
    expect(out[0].label).toBe("lineas.a = cab.x AND lineas.b = cab.y");
  });

  it("sin clave entre las dos tablas no propone nada", () => {
    expect(joinConditions(ref("otra"), [ref("pedidos")], keys, publicSchema)).toEqual([]);
  });

  it("dos claves entre las mismas tablas dan dos opciones", () => {
    const dos = [fk("vuelos", ["origen"], "aeropuertos", ["id"]), fk("vuelos", ["destino"], "aeropuertos", ["id"])];
    expect(joinConditions(ref("aeropuertos", "a"), [ref("vuelos", "v")], dos, publicSchema)).toHaveLength(2);
  });

  it("no cruza esquemas distintos con el mismo nombre", () => {
    const otro = [{ ...fk("pedidos", ["c"], "clientes", ["id"]), refSchema: "ventas" }];
    expect(joinConditions(ref("clientes"), [ref("pedidos")], otro, publicSchema)).toEqual([]);
  });

  it("cita lo que lo necesita", () => {
    expect(ident("Mi Col")).toBe('"Mi Col"');
    expect(ident("id")).toBe("id");
  });
});
