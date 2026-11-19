import { describe, expect, it } from "vitest";
import { lintStatement } from "./sql-lint";

const rules = (sql: string, offset = 0) => lintStatement(sql, offset).map((f) => f.rule);

describe("lintStatement", () => {
  it("avisa de = NULL, <> NULL y != NULL", () => {
    expect(rules("SELECT 1 FROM t WHERE a = NULL")).toContain("null-comparison");
    expect(rules("SELECT 1 FROM t WHERE a <> NULL")).toContain("null-comparison");
    expect(rules("SELECT 1 FROM t WHERE a != NULL")).toContain("null-comparison");
  });

  it("marca el operador y el NULL con la posición absoluta", () => {
    const sql = "SELECT 1 FROM t WHERE a <> NULL";
    const [finding] = lintStatement(sql, 100);
    expect(sql.slice(finding.from - 100, finding.to - 100)).toBe("<> NULL");
  });

  it("no avisa de IS NULL ni de la asignación SET x = NULL", () => {
    expect(rules("SELECT 1 FROM t WHERE a IS NULL")).toEqual([]);
    expect(rules("UPDATE t SET a = NULL WHERE id = 1")).toEqual([]);
    expect(rules("INSERT INTO t VALUES (NULL)")).toEqual([]);
  });

  it("no mira adentro de cadenas ni comentarios", () => {
    expect(rules("SELECT 'a = NULL' FROM t")).not.toContain("null-comparison");
    expect(rules("SELECT 1 -- a = NULL\nFROM t LIMIT 1")).toEqual([]);
  });

  it("avisa de NOT IN con subconsulta pero no con una lista", () => {
    expect(rules("SELECT 1 FROM t WHERE id NOT IN (SELECT id FROM u)")).toContain("not-in-subquery");
    expect(rules("SELECT 1 FROM t WHERE id NOT IN (1, 2, 3)")).not.toContain("not-in-subquery");
  });

  it("avisa de un LIKE con comodín inicial, y no de uno sin él", () => {
    expect(rules("SELECT 1 FROM t WHERE n LIKE '%ana'")).toContain("leading-wildcard");
    expect(rules("SELECT 1 FROM t WHERE n ILIKE '%ana%'")).toContain("leading-wildcard");
    expect(rules("SELECT 1 FROM t WHERE n LIKE 'ana%'")).not.toContain("leading-wildcard");
    expect(rules("SELECT 1 FROM t WHERE n LIKE '%'")).not.toContain("leading-wildcard");
  });

  it("avisa de una función o un cast sobre la columna del WHERE", () => {
    expect(rules("SELECT 1 FROM t WHERE lower(nombre) = 'x'")).toContain("function-on-column");
    expect(rules("SELECT 1 FROM t WHERE date(creado) = '2024-01-01'")).toContain("function-on-column");
    expect(rules("SELECT 1 FROM t WHERE a.nombre::text = 'x'")).not.toContain("cast-on-column");
    expect(rules("SELECT 1 FROM t WHERE id::text = '5'")).toContain("cast-on-column");
  });

  it("no avisa si la función no está comparada o fuera del WHERE", () => {
    expect(rules("SELECT lower(nombre) FROM t LIMIT 1")).toEqual([]);
    expect(rules("SELECT 1 FROM t WHERE x = lower('A')")).not.toContain("function-on-column");
    expect(rules("SELECT 1 FROM t WHERE lower(a) || b = 'x'")).not.toContain("function-on-column");
  });

  it("avisa de SELECT * sin LIMIT y no con LIMIT, agregados ni DISTINCT", () => {
    expect(rules("SELECT * FROM t")).toContain("select-star");
    expect(rules("SELECT a.*, b.x FROM a JOIN b ON true")).toContain("select-star");
    expect(rules("SELECT * FROM t LIMIT 10")).toEqual([]);
    expect(rules("SELECT DISTINCT * FROM t")).toEqual([]);
    expect(rules("SELECT count(*) FROM t")).toEqual([]);
    expect(rules("SELECT * FROM t GROUP BY a")).toEqual([]);
    expect(rules("SELECT 2 * 3")).toEqual([]);
  });

  it("delega el UPDATE y el DELETE sin WHERE en statement-risk", () => {
    expect(rules("UPDATE t SET a = 1")).toEqual(["update-all"]);
    expect(rules("DELETE FROM t")).toEqual(["delete-all"]);
    expect(rules("DELETE FROM t WHERE id = 1")).toEqual([]);
    expect(rules("UPDATE t SET a = 1 WHERE true")).toEqual(["update-all"]);
  });

  it("calla ante DDL, texto sin cerrar y cuerpos de función", () => {
    expect(rules("CREATE FUNCTION f() RETURNS int AS $$ SELECT * FROM t WHERE a = NULL $$ LANGUAGE sql")).toEqual([]);
    expect(rules("SELECT * FROM t WHERE a = 'abc")).toEqual([]);
    expect(rules("")).toEqual([]);
  });

  it("devuelve los hallazgos ordenados y sin repetir", () => {
    const found = lintStatement("SELECT * FROM t WHERE a = NULL", 0);
    expect(found.map((f) => f.from)).toEqual([...found.map((f) => f.from)].sort((a, b) => a - b));
    expect(new Set(found.map((f) => `${f.rule}@${f.from}`)).size).toBe(found.length);
  });
});
