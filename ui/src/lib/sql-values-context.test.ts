import { describe, expect, it } from "vitest";
import { valueContextAt } from "./sql-values-context";
import type { SchemaRelation } from "./ipc";

let oid = 0;
function relation(schema: string, name: string, columns: string[]): SchemaRelation {
  oid += 1;
  return { oid, schema, name, columns: columns.map((c) => ({ name: c, typeName: "text" })) };
}

const RELATIONS = [
  relation("public", "clientes", ["id", "nombre", "estado"]),
  relation("public", "pedidos", ["id", "cliente_id", "estado"]),
];

function context(marked: string) {
  const pos = marked.indexOf("|");
  const text = marked.replace("|", "");
  const result = valueContextAt(text, pos, RELATIONS);
  return result === null ? null : { ...result, typed: text.slice(result.from, pos) };
}

describe("valueContextAt", () => {
  it("después de un igual, con o sin comilla", () => {
    expect(context("SELECT 1 FROM clientes WHERE nombre = |")).toMatchObject({
      table: "clientes",
      column: "nombre",
      insideQuote: false,
      typed: "",
    });
    expect(context("SELECT 1 FROM clientes WHERE nombre = '|")).toMatchObject({
      column: "nombre",
      insideQuote: true,
      typed: "",
    });
  });

  it("lo ya escrito del valor queda entre from y el cursor", () => {
    expect(context("SELECT 1 FROM clientes WHERE nombre = 'Ana|")).toMatchObject({ insideQuote: true, typed: "Ana" });
    expect(context("SELECT 1 FROM clientes WHERE nombre = An|")).toMatchObject({ insideQuote: false, typed: "An" });
  });

  it("con otros operadores y LIKE", () => {
    expect(context("SELECT 1 FROM clientes WHERE nombre <> '|")).toMatchObject({ column: "nombre" });
    expect(context("SELECT 1 FROM clientes WHERE nombre != '|")).toMatchObject({ column: "nombre" });
    expect(context("SELECT 1 FROM clientes WHERE nombre LIKE '|")).toMatchObject({ column: "nombre" });
    expect(context("SELECT 1 FROM clientes WHERE nombre NOT ILIKE '|")).toMatchObject({ column: "nombre" });
  });

  it("dentro de una lista IN, en el primer valor y en los siguientes", () => {
    expect(context("SELECT 1 FROM clientes WHERE estado IN ('|")).toMatchObject({ column: "estado" });
    expect(context("SELECT 1 FROM clientes WHERE estado IN ('activo', '|")).toMatchObject({ column: "estado" });
    expect(context("SELECT 1 FROM clientes WHERE estado NOT IN ('a', 'b', |")).toMatchObject({ column: "estado" });
  });

  it("resuelve la tabla por el alias", () => {
    expect(context("SELECT 1 FROM clientes c JOIN pedidos p ON true WHERE p.estado = '|")).toMatchObject({
      table: "pedidos",
      column: "estado",
    });
  });

  it("una columna que está en dos tablas sin calificar es ambigua", () => {
    expect(context("SELECT 1 FROM clientes c JOIN pedidos p ON true WHERE estado = '|")).toBeNull();
  });

  it("una columna que el catálogo no conoce no resuelve", () => {
    expect(context("SELECT 1 FROM clientes WHERE otra = '|")).toBeNull();
    expect(context("SELECT 1 FROM desconocida WHERE estado = '|")).toBeNull();
  });

  it("no es un valor de columna: casts, funciones, asignaciones y cadenas sueltas", () => {
    expect(context("SELECT 1 FROM clientes WHERE estado::text = '|")).toBeNull();
    expect(context("SELECT 1 FROM clientes WHERE lower(estado) = '|")).toBeNull();
    expect(context("SELECT '|")).toBeNull();
    expect(context("SELECT 1 FROM clientes WHERE estado IS NULL AND |")).toBeNull();
    expect(context("SELECT 1 FROM clientes WHERE estado IN (1, 2) AND nombre|")).toBeNull();
  });

  it("una cadena ya cerrada y el cursor justo después no cuentan como estar adentro", () => {
    expect(context("SELECT 1 FROM clientes WHERE nombre = 'Ana'|")).toBeNull();
  });

  it("editando en el medio de un literal cerrado", () => {
    expect(context("SELECT 1 FROM clientes WHERE nombre = 'A|na'")).toMatchObject({
      insideQuote: true,
      typed: "A",
    });
  });
});
