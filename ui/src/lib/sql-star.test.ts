import { describe, expect, it } from "vitest";
import { expandStar } from "./sql-star";
import type { SchemaRelation } from "./ipc";

let oid = 0;
function relation(schema: string, name: string, columns: string[]): SchemaRelation {
  oid += 1;
  return { oid, schema, name, columns: columns.map((c) => ({ name: c, typeName: "text" })) };
}

const RELATIONS = [
  relation("public", "clientes", ["id", "nombre", "Creado"]),
  relation("public", "pedidos", ["id", "cliente_id", "total"]),
];

/** Expande con el cursor en `|` y devuelve el texto resultante, o `null`. */
function expand(marked: string): string | null {
  const pos = marked.indexOf("|");
  const text = marked.replace("|", "");
  const result = expandStar(text, pos, RELATIONS);
  return result === null ? null : text.slice(0, result.from) + result.insert + text.slice(result.to);
}

describe("expandStar", () => {
  it("expande * con las columnas de la tabla, alineadas bajo la primera", () => {
    expect(expand("SELECT *| FROM pedidos")).toBe(
      "SELECT id,\n       cliente_id,\n       total FROM pedidos",
    );
  });

  it("con el cursor antes del asterisco también", () => {
    expect(expand("SELECT |* FROM pedidos")).not.toBeNull();
  });

  it("si el * está solo en su línea, usa la sangría de la línea", () => {
    expect(expand("SELECT\n  *|\nFROM pedidos")).toBe("SELECT\n  id,\n  cliente_id,\n  total\nFROM pedidos");
  });

  it("cita lo que lo necesita", () => {
    expect(expand("SELECT *| FROM clientes")).toContain('"Creado"');
  });

  it("con varias tablas califica cada columna con el alias o el nombre", () => {
    expect(expand("SELECT *| FROM clientes c JOIN pedidos ON true")).toBe(
      'SELECT c.id,\n       c.nombre,\n       c."Creado",\n       pedidos.id,\n       pedidos.cliente_id,\n       pedidos.total FROM clientes c JOIN pedidos ON true',
    );
  });

  it("alias.* expande solo esa tabla y deja la calificación", () => {
    expect(expand("SELECT p.*| FROM clientes c JOIN pedidos p ON true")).toBe(
      "SELECT p.id,\n       p.cliente_id,\n       p.total FROM clientes c JOIN pedidos p ON true",
    );
  });

  it("mantiene lo que ya estaba en la lista", () => {
    expect(expand("SELECT c.nombre, p.*| FROM clientes c JOIN pedidos p ON true")).toContain("c.nombre, p.id");
  });

  it("sirve en RETURNING", () => {
    expect(expand("DELETE FROM pedidos WHERE id = 1 RETURNING *|")).toBe(
      "DELETE FROM pedidos WHERE id = 1 RETURNING id,\n                                           cliente_id,\n                                           total",
    );
  });

  it("no expande count(*), una multiplicación ni un * que no es de la lista", () => {
    expect(expand("SELECT count(*|) FROM pedidos")).toBeNull();
    expect(expand("SELECT 2 *| 3")).toBeNull();
    expect(expand("SELECT a *| b FROM pedidos")).toBeNull();
    expect(expand("SELECT total FROM pedidos WHERE total *| 2 > 1")).toBeNull();
  });

  it("no expande dentro de EXISTS", () => {
    expect(expand("SELECT 1 WHERE EXISTS (SELECT *| FROM pedidos)")).toBeNull();
  });

  it("una subconsulta usa solo sus propias tablas", () => {
    expect(expand("SELECT 1 FROM clientes WHERE id IN (SELECT *| FROM pedidos)")).toBe(
      "SELECT 1 FROM clientes WHERE id IN (SELECT id,\n                                           cliente_id,\n                                           total FROM pedidos)",
    );
  });

  it("calla si alguna tabla no se conoce", () => {
    expect(expand("SELECT *| FROM desconocida")).toBeNull();
    expect(expand("SELECT *| FROM generate_series(1, 3)")).toBeNull();
    expect(expand("SELECT *| FROM (SELECT 1) x")).toBeNull();
    expect(expand("SELECT *| FROM clientes c JOIN desconocida d ON true")).toBeNull();
  });

  it("calla si el alias no existe, sin FROM o con una cadena sin cerrar", () => {
    expect(expand("SELECT z.*| FROM pedidos p")).toBeNull();
    expect(expand("SELECT *|")).toBeNull();
    expect(expand("SELECT * FROM pedidos WHERE x = 'a|")).toBeNull();
  });

  it("expande el * de una CTE", () => {
    expect(expand("WITH v AS (SELECT id, total FROM pedidos) SELECT *| FROM v")).toBe(
      "WITH v AS (SELECT id, total FROM pedidos) SELECT id,\n                                                 total FROM v",
    );
  });

  it("devuelve cuántas columnas puso", () => {
    const text = "SELECT * FROM pedidos";
    expect(expandStar(text, 8, RELATIONS)?.count).toBe(3);
  });

  it("solo toma la sentencia del cursor", () => {
    expect(expand("SELECT 1 FROM clientes; SELECT *| FROM pedidos")).toContain("cliente_id");
  });
});
