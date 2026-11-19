import { describe, expect, it } from "vitest";
import { RELATIONS } from "./fixtures";
import { peekTarget } from "./table-peek";

describe("peekTarget", () => {
  it("resuelve una tabla del FROM", () => {
    const sql = "SELECT * FROM clientes";
    const found = peekTarget(sql, sql.indexOf("clientes") + 2, RELATIONS);
    expect(found?.relation.name).toBe("clientes");
  });

  it("resuelve un alias a su tabla", () => {
    const sql = "SELECT c.id FROM clientes c";
    const found = peekTarget(sql, sql.indexOf("c.id"), RELATIONS);
    // `c` solo es el alias: la tarjeta es la de la tabla.
    expect(found?.relation.name).toBe("clientes");
  });

  it("calla ante una columna: ya tiene su propio globo", () => {
    const sql = "SELECT nombre FROM clientes";
    expect(peekTarget(sql, sql.indexOf("nombre") + 2, RELATIONS)).toBeNull();
  });

  it("calla ante una CTE aunque se llame como una tabla real", () => {
    const sql = "WITH clientes AS (SELECT 1 AS x) SELECT * FROM clientes";
    expect(peekTarget(sql, sql.lastIndexOf("clientes") + 2, RELATIONS)).toBeNull();
  });

  it("calla ante lo que no resuelve", () => {
    const sql = "SELECT * FROM fantasma";
    expect(peekTarget(sql, sql.indexOf("fantasma") + 2, RELATIONS)).toBeNull();
  });
});
