import { describe, expect, it } from "vitest";
import { sizeTarget, withLimit } from "./select-size";

describe("sizeTarget", () => {
  it("reconoce un SELECT simple sobre una tabla", () => {
    expect(sizeTarget("SELECT * FROM eventos")).toEqual({ schema: null, table: "eventos" });
    expect(sizeTarget("select a, b from public.eventos e where e.x = 1 order by a;")).toEqual({
      schema: "public",
      table: "eventos",
    });
  });

  it("no avisa con LIMIT, agregados, joins ni varias sentencias", () => {
    for (const sql of [
      "SELECT * FROM t LIMIT 10",
      "SELECT count(*) FROM t",
      "SELECT a FROM t GROUP BY a",
      "SELECT DISTINCT a FROM t",
      "SELECT * FROM a JOIN b ON a.id = b.id",
      "SELECT * FROM a; SELECT * FROM b",
      "UPDATE t SET a = 1",
      "SELECT * FROM (SELECT 1) x",
      "SELECT 1",
    ]) {
      expect(sizeTarget(sql), sql).toBeNull();
    }
  });

  it("una palabra de agregado dentro de una cadena o comentario no lo descarta", () => {
    expect(sizeTarget("SELECT * FROM t WHERE n = 'limit 5' -- count(x)")).toEqual({ schema: null, table: "t" });
  });
});

describe("withLimit", () => {
  it("agrega el LIMIT al final aunque haya punto y coma o un comentario", () => {
    expect(withLimit("SELECT * FROM t;", 1000)).toBe("SELECT * FROM t\nLIMIT 1000");
    expect(withLimit("SELECT * FROM t -- nota", 1000)).toBe("SELECT * FROM t -- nota\nLIMIT 1000");
  });
});
