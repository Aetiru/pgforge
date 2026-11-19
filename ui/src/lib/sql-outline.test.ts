import { describe, expect, it } from "vitest";
import { outline, rangeFrom, rangeUntil } from "./sql-outline";

describe("outline", () => {
  it("parte un script en sentencias con su tipo y su línea", () => {
    const items = outline("SELECT 1;\n\nCREATE OR REPLACE FUNCTION f() RETURNS int AS $$ SELECT 1; $$ LANGUAGE sql;\nUPDATE t SET a = 1");
    expect(items.map((i) => i.kind)).toEqual(["SELECT", "CREATE FUNCTION", "UPDATE"]);
    expect(items.map((i) => i.line)).toEqual([1, 3, 4]);
  });

  it("no corta en los ; de cadenas, comentarios ni $$", () => {
    const items = outline("SELECT 'a;b'; -- uno; dos\nSELECT $f$ x; y $f$; /* ; */ SELECT 3");
    expect(items).toHaveLength(3);
  });

  it("no corta adentro de BEGIN ATOMIC … END, ni de un CASE de adentro", () => {
    const sql =
      "CREATE FUNCTION f(x int) RETURNS int LANGUAGE sql BEGIN ATOMIC SELECT CASE WHEN x > 0 THEN 1 ELSE 0 END; SELECT 2; END; SELECT 3";
    const items = outline(sql);
    expect(items).toHaveLength(2);
    expect(items[0].kind).toBe("CREATE FUNCTION");
    expect(sql.slice(items[1].from)).toBe("SELECT 3");
  });

  it("usa como título el comentario de justo arriba, y si no la primera línea", () => {
    const items = outline("-- Ventas del mes\nSELECT sum(total)\nFROM pedidos;\n\n-- lejos\n\nSELECT 2");
    expect(items[0].title).toBe("Ventas del mes");
    expect(items[1].title).toBe("SELECT 2");
  });

  it("recorta los títulos largos a 80", () => {
    const [item] = outline(`SELECT ${"a, ".repeat(60)}b FROM t`);
    expect(item.title.length).toBeLessThanOrEqual(80);
    expect(item.title.endsWith("…")).toBe(true);
  });

  it("el verbo de una consulta con CTE es el de la sentencia principal", () => {
    expect(outline("WITH x AS (SELECT 1) DELETE FROM t")[0].kind).toBe("DELETE");
    expect(outline("WITH x AS (SELECT 1) SELECT * FROM x")[0].kind).toBe("SELECT");
  });

  it("a una sentencia con ; le incluye el punto y coma, y a la última sin él no le inventa", () => {
    const sql = "SELECT 1; SELECT 2";
    const items = outline(sql);
    expect(sql.slice(items[0].from, items[0].to)).toBe("SELECT 1;");
    expect(sql.slice(items[1].from, items[1].to)).toBe("SELECT 2");
  });

  it("un script vacío o de solo comentarios no tiene sentencias", () => {
    expect(outline("")).toEqual([]);
    expect(outline("-- nada\n/* nada */")).toEqual([]);
  });

  it("tolera una cadena sin cerrar", () => {
    expect(outline("SELECT 'abc")).toHaveLength(1);
  });
});

describe("rangeUntil y rangeFrom", () => {
  const sql = "SELECT 1;\nSELECT 2;\nSELECT 3;";
  const at = (n: number) => sql.indexOf(`SELECT ${n}`) + 3;

  it("hasta el final de la sentencia del cursor", () => {
    expect(rangeUntil(sql, at(2))).toBe("SELECT 1;\nSELECT 2;");
    expect(rangeUntil(sql, at(1))).toBe("SELECT 1;");
  });

  it("desde el comienzo de la sentencia del cursor", () => {
    expect(rangeFrom(sql, at(2))).toBe("SELECT 2;\nSELECT 3;");
    expect(rangeFrom(sql, at(3))).toBe("SELECT 3;");
  });

  it("con el cursor entre dos sentencias, hasta la anterior y desde la siguiente", () => {
    const gap = sql.indexOf("\n") + 0; // justo después del primer ;
    const between = "SELECT 1;\n\n\nSELECT 2;";
    const cursor = between.indexOf("\n\n") + 1;
    expect(rangeUntil(between, cursor)).toBe("SELECT 1;");
    expect(rangeFrom(between, cursor)).toBe("SELECT 2;");
    expect(gap).toBeGreaterThan(0);
  });

  it("sin nada que ejecutar devuelve vacío", () => {
    expect(rangeUntil("-- solo\n", 3)).toBe("");
    expect(rangeFrom("SELECT 1;\n  ", 12)).toBe("");
  });
});
