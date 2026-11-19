import { describe, expect, it } from "vitest";
import { isWatchable } from "./watch-guard";

describe("isWatchable", () => {
  it("acepta lecturas simples, con comentarios y punto y coma final", () => {
    expect(isWatchable("SELECT * FROM cola WHERE estado = 'pendiente';")).toBe(true);
    expect(isWatchable("-- nota\n  with x as (select 1) select * from x")).toBe(true);
    expect(isWatchable("SHOW max_connections")).toBe(true);
  });

  it("rechaza escrituras, también las escondidas en un WITH", () => {
    expect(isWatchable("UPDATE t SET a = 1")).toBe(false);
    expect(isWatchable("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d")).toBe(false);
    expect(isWatchable("SELECT nextval('s')")).toBe(false);
  });

  it("rechaza más de una sentencia", () => {
    expect(isWatchable("SELECT 1; SELECT 2")).toBe(false);
  });

  it("una palabra de escritura adentro de una cadena o un comentario no cuenta", () => {
    expect(isWatchable("SELECT 'update me' AS nota -- delete\nFROM t")).toBe(true);
  });

  it("no acepta lo que no es una lectura", () => {
    expect(isWatchable("")).toBe(false);
    expect(isWatchable("EXPLAIN ANALYZE SELECT 1")).toBe(false);
  });
});
