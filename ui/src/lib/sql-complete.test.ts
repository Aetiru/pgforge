import { describe, expect, it } from "vitest";
import { columnOptions, tablesInScope } from "./sql-complete";
import type { SchemaRelation } from "./ipc";

const RELATIONS: SchemaRelation[] = [
  { schema: "public", name: "clientes", columns: ["id", "nombre", "creado"] },
  { schema: "public", name: "pedidos", columns: ["id", "cliente_id", "total"] },
  { schema: "ventas", name: "clientes", columns: ["id", "razon_social"] },
];

/** Dónde está el cursor se marca con `|` para no contar caracteres a mano en cada caso. */
function at(marked: string) {
  const cursor = marked.indexOf("|");
  return { sql: marked.replace("|", ""), cursor };
}

describe("tablesInScope", () => {
  it("encuentra la tabla del FROM sin calificar", () => {
    const { sql, cursor } = at("SELECT | FROM clientes");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);
  });

  it("se queda con el alias cuando lo hay, con AS y sin AS", () => {
    const { sql, cursor } = at("SELECT | FROM public.clientes AS c JOIN pedidos p ON p.id = c.id");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: "public", name: "clientes", alias: "c" },
      { schema: null, name: "pedidos", alias: "p" },
    ]);
  });

  it("parte la lista del FROM por las comas de primer nivel", () => {
    const { sql, cursor } = at("SELECT | FROM clientes c, pedidos");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: "c" },
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("no toma como alias la palabra que cierra la lista", () => {
    const { sql, cursor } = at("SELECT | FROM clientes WHERE id = 1");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);
  });

  it("ignora las subconsultas, cuyas columnas no están en el catálogo", () => {
    const { sql, cursor } = at("SELECT | FROM (SELECT 1) t, pedidos");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("solo mira la sentencia donde está el cursor", () => {
    const { sql, cursor } = at("SELECT * FROM clientes;\nSELECT | FROM pedidos;");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("toma la tabla de un UPDATE y la de un INSERT INTO", () => {
    const update = at("UPDATE clientes SET | = 1");
    expect(tablesInScope(update.sql, update.cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);

    const insert = at("INSERT INTO pedidos (|)");
    expect(tablesInScope(insert.sql, insert.cursor)).toEqual([
      { schema: null, name: "pedidos", alias: null },
    ]);
  });

  it("saca los nombres de adentro de las comillas", () => {
    const { sql, cursor } = at('SELECT | FROM "Mi Esquema"."Mi Tabla" t');
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: "Mi Esquema", name: "Mi Tabla", alias: "t" },
    ]);
  });

  it("no se confunde con un FROM adentro de un comentario", () => {
    const { sql, cursor } = at("SELECT | -- FROM pedidos\nFROM clientes");
    expect(tablesInScope(sql, cursor)).toEqual([
      { schema: null, name: "clientes", alias: null },
    ]);
  });
});

describe("columnOptions", () => {
  it("ofrece las columnas de la tabla del FROM con el alias al lado", () => {
    const refs = [{ schema: null, name: "clientes", alias: "c" }];
    expect(columnOptions(RELATIONS, refs)).toEqual([
      { label: "id", detail: "c" },
      { label: "nombre", detail: "c" },
      { label: "creado", detail: "c" },
    ]);
  });

  it("desempata por esquema cuando el nombre está repetido", () => {
    const refs = [{ schema: "ventas", name: "clientes", alias: null }];
    expect(columnOptions(RELATIONS, refs).map((option) => option.label)).toEqual([
      "id",
      "razon_social",
    ]);
  });

  it("repite la columna que está en dos tablas, una vez por cada una", () => {
    const refs = [
      { schema: null, name: "clientes", alias: "c" },
      { schema: null, name: "pedidos", alias: "p" },
    ];
    const id = columnOptions(RELATIONS, refs).filter((option) => option.label === "id");
    expect(id).toEqual([
      { label: "id", detail: "c" },
      { label: "id", detail: "p" },
    ]);
  });

  it("no ofrece nada de una tabla que el catálogo no conoce", () => {
    const refs = [{ schema: null, name: "inventada", alias: null }];
    expect(columnOptions(RELATIONS, refs)).toEqual([]);
  });
});
