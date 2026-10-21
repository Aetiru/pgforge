import { describe, expect, it } from "vitest";
import { addToWhere, clause, escapeLike, quoteIdent, quoteLiteral } from "./column-filter";

const COLUMNS = ["id", "Nombre", 'a"b', "datos"];

describe("quoteLiteral", () => {
  it("duplica las comillas simples", () => {
    expect(quoteLiteral("O'Brien")).toBe("'O''Brien'");
  });

  it("no deja salir un valor de su literal", () => {
    expect(quoteLiteral("x'; DROP TABLE t; --")).toBe("'x''; DROP TABLE t; --'");
  });

  it("usa E'' y duplica la barra invertida", () => {
    expect(quoteLiteral("a\\'b")).toBe("E'a\\\\''b'");
  });

  it("descarta el byte nulo", () => {
    expect(quoteLiteral("a\u0000b")).toBe("'ab'");
  });
});

describe("quoteIdent / escapeLike", () => {
  it("cita siempre y duplica comillas dobles", () => {
    expect(quoteIdent('a"b')).toBe('"a""b"');
  });

  it("escapa los comodines de LIKE", () => {
    expect(escapeLike("50%_x\\")).toBe("50\\%\\_x\\\\");
  });
});

describe("clause", () => {
  it("arma igual y distinto", () => {
    expect(clause({ column: "id", op: "eq", value: "5" }, COLUMNS)).toEqual({ sql: `"id" = '5'` });
    expect(clause({ column: "Nombre", op: "neq", value: "x" }, COLUMNS)).toEqual({
      sql: `"Nombre" IS DISTINCT FROM 'x'`,
    });
  });

  it("arma nulo y no nulo sin valor", () => {
    expect(clause({ column: "id", op: "isNull" }, COLUMNS)).toEqual({ sql: `"id" IS NULL` });
    expect(clause({ column: "id", op: "notNull" }, COLUMNS)).toEqual({ sql: `"id" IS NOT NULL` });
  });

  it("contiene escapa comodines y compara como texto", () => {
    expect(clause({ column: "id", op: "contains", value: "5%" }, COLUMNS)).toEqual({
      sql: `"id"::text ILIKE E'%5\\\\%%'`,
    });
  });

  it("rechaza una columna que no está en la tabla", () => {
    expect(clause({ column: "x; DROP", op: "isNull" }, COLUMNS)).toHaveProperty("error");
  });

  it("cita un nombre con comillas", () => {
    expect(clause({ column: 'a"b', op: "isNull" }, COLUMNS)).toEqual({ sql: `"a""b" IS NULL` });
  });

  it("compara json como texto", () => {
    expect(clause({ column: "datos", op: "eq", value: "{}" }, COLUMNS, ["int4", "text", "text", "json"])).toEqual({
      sql: `"datos"::text = '{}'`,
    });
  });

  it("pide texto para contiene", () => {
    expect(clause({ column: "id", op: "contains", value: "" }, COLUMNS)).toHaveProperty("error");
  });
});

describe("addToWhere", () => {
  it("sin filtro previo devuelve la condición sola", () => {
    expect(addToWhere("  ", `"a" = '1'`)).toBe(`"a" = '1'`);
  });

  it("envuelve lo previo para que un OR no cambie de sentido", () => {
    expect(addToWhere("a = 1 OR b = 2;", `"c" IS NULL`)).toBe(`(a = 1 OR b = 2) AND "c" IS NULL`);
  });
});
