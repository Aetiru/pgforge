import { describe, expect, it } from "vitest";
import { assessRisks, typedMatches, type RiskKind } from "./statement-risk";

const kinds = (sql: string): RiskKind[] => assessRisks(sql).map((risk) => risk.kind);

describe("assessRisks: lo que pide escribir el nombre del servidor", () => {
  it("detecta DROP, TRUNCATE y los DELETE/UPDATE sin WHERE", () => {
    expect(kinds("DROP TABLE clientes")).toEqual(["drop"]);
    expect(kinds("drop index concurrently if exists ix")).toEqual(["drop"]);
    expect(kinds("TRUNCATE clientes, pedidos CASCADE")).toEqual(["truncate"]);
    expect(kinds("DELETE FROM clientes")).toEqual(["delete-all"]);
    expect(kinds("UPDATE clientes SET estado = 'baja'")).toEqual(["update-all"]);
  });

  it("no molesta con lo que sí filtra", () => {
    expect(kinds("DELETE FROM clientes WHERE id = 3")).toEqual([]);
    expect(kinds("UPDATE clientes SET estado = 'baja' WHERE id = 3")).toEqual([]);
    expect(kinds("SELECT * FROM clientes")).toEqual([]);
    expect(kinds("INSERT INTO t VALUES (1)")).toEqual([]);
    expect(kinds("CREATE TABLE t (id int)")).toEqual([]);
  });

  it("un ALTER que elimina columnas pierde datos igual que un DROP", () => {
    expect(kinds("ALTER TABLE t DROP COLUMN c")).toEqual(["drop"]);
    expect(kinds("ALTER TABLE t ADD COLUMN c int")).toEqual([]);
    expect(kinds("ALTER TABLE t ADD FOREIGN KEY (a) REFERENCES u (id) ON DELETE CASCADE")).toEqual(
      [],
    );
  });

  it("ignora comentarios, cadenas e identificadores citados", () => {
    expect(kinds("-- DROP TABLE t\nSELECT 1")).toEqual([]);
    expect(kinds("/* TRUNCATE t */ SELECT 1")).toEqual([]);
    expect(kinds("/* a /* anidado */ DROP TABLE t */ SELECT 1")).toEqual([]);
    expect(kinds("SELECT 'DELETE FROM t'")).toEqual([]);
    expect(kinds("SELECT 'it''s; DROP TABLE t'")).toEqual([]);
    expect(kinds('SELECT "drop" FROM t')).toEqual([]);
    expect(kinds("INSERT INTO log VALUES ($$ DROP TABLE t; $$)")).toEqual([]);
    expect(kinds("INSERT INTO log VALUES ($f$ TRUNCATE t $f$)")).toEqual([]);
  });

  it("un comentario delante no esconde la sentencia", () => {
    expect(kinds("-- limpiar\n/* ojo */ DELETE FROM t")).toEqual(["delete-all"]);
  });

  it("el WHERE tiene que ser de la sentencia y no de una subconsulta", () => {
    expect(
      kinds("DELETE FROM t USING (SELECT id FROM u WHERE activo) s WHERE t.id = s.id"),
    ).toEqual([]);
    expect(kinds("DELETE FROM t USING (SELECT id FROM u WHERE activo) s")).toEqual(["delete-all"]);
    expect(kinds("UPDATE t SET a = (SELECT max(a) FROM u WHERE b = 1)")).toEqual(["update-all"]);
  });

  it("WHERE true y WHERE 1 = 1 filtran lo mismo que nada", () => {
    expect(kinds("DELETE FROM t WHERE true")).toEqual(["delete-all"]);
    expect(kinds("DELETE FROM t WHERE 1 = 1")).toEqual(["delete-all"]);
    expect(kinds("UPDATE t SET a = 1 WHERE TRUE RETURNING *")).toEqual(["update-all"]);
    expect(kinds("DELETE FROM t WHERE true AND id = 3")).toEqual([]);
  });

  it("ve la sentencia de datos dentro de una CTE", () => {
    expect(kinds("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d")).toEqual(["delete-all"]);
    expect(kinds("WITH d AS (DELETE FROM t WHERE id = 1 RETURNING *) SELECT * FROM d")).toEqual([]);
    expect(kinds("WITH x AS (SELECT 1) DELETE FROM t")).toEqual(["delete-all"]);
    expect(kinds("WITH x AS (SELECT 1) UPDATE t SET a = 1 WHERE a IN (SELECT * FROM x)")).toEqual(
      [],
    );
  });

  it("no toma por UPDATE lo que no lo es", () => {
    expect(kinds("SELECT * FROM t FOR UPDATE")).toEqual([]);
    expect(kinds("SELECT * FROM t FOR NO KEY UPDATE")).toEqual([]);
    expect(kinds("INSERT INTO t VALUES (1) ON CONFLICT (id) DO UPDATE SET a = 2")).toEqual([]);
    expect(kinds("CREATE TRIGGER x AFTER UPDATE ON t FOR EACH ROW EXECUTE FUNCTION f()")).toEqual(
      [],
    );
    expect(kinds("GRANT UPDATE ON t TO r")).toEqual([]);
  });

  it("EXPLAIN ANALYZE ejecuta, EXPLAIN a secas no", () => {
    expect(kinds("EXPLAIN DELETE FROM t")).toEqual([]);
    expect(kinds("EXPLAIN ANALYZE DELETE FROM t")).toEqual(["delete-all"]);
    expect(kinds("EXPLAIN (ANALYZE, BUFFERS) UPDATE t SET a = 1")).toEqual(["update-all"]);
  });

  it("mira cada sentencia del script y dice cuál", () => {
    const risks = assessRisks("SELECT 1;\nDELETE FROM t WHERE id = 1;\nTRUNCATE t;\nDROP TABLE u;");
    expect(risks.map((risk) => [risk.index, risk.kind])).toEqual([
      [2, "truncate"],
      [3, "drop"],
    ]);
    expect(risks[0].statement).toBe("TRUNCATE t");
  });

  it("lee dentro de un bloque DO", () => {
    expect(kinds("DO $$ BEGIN DELETE FROM t; END $$")).toEqual(["delete-all"]);
    expect(kinds("DO $$ BEGIN IF true THEN DELETE FROM t; END IF; END $$")).toEqual(["delete-all"]);
    expect(kinds("DO $$ BEGIN DELETE FROM t WHERE id = 1; END $$")).toEqual([]);
    expect(kinds("DO $$ BEGIN PERFORM 1; END $$")).toEqual([]);
    expect(kinds("DO $x$ BEGIN DROP TABLE t; END $x$")).toEqual(["drop"]);
  });

  it("ante la duda, más fuerte: SQL dinámico y texto sin cerrar", () => {
    expect(kinds("DO $$ BEGIN EXECUTE 'DROP TABLE ' || quote_ident(n); END $$")).toEqual([
      "unsure",
    ]);
    expect(kinds("DO")).toEqual(["unsure"]);
    expect(kinds("SELECT 'sin cerrar")).toEqual(["unsure"]);
    expect(kinds("SELECT 1 /* sin cerrar")).toEqual(["unsure"]);
    expect(kinds("SELECT $$ sin cerrar")).toEqual(["unsure"]);
  });

  it("los parámetros $1 no se toman por cuerpos de dólares", () => {
    expect(kinds("DELETE FROM t WHERE id = $1")).toEqual([]);
    expect(kinds("DELETE FROM t WHERE a = $1 OR b = $2; DROP TABLE u")).toEqual(["drop"]);
  });

  it("las cadenas E'' escapan con barra invertida", () => {
    expect(kinds("SELECT E'it\\'s; DROP TABLE t'")).toEqual([]);
  });
});

describe("typedMatches", () => {
  it("pide el nombre exacto, ignorando los bordes", () => {
    expect(typedMatches("prod-ventas", "prod-ventas")).toBe(true);
    expect(typedMatches("  prod-ventas ", "prod-ventas")).toBe(true);
    expect(typedMatches("Prod-Ventas", "prod-ventas")).toBe(false);
    expect(typedMatches("prod", "prod-ventas")).toBe(false);
    expect(typedMatches("", "prod-ventas")).toBe(false);
  });

  it("un servidor sin nombre no habilita nada escribiendo nada", () => {
    expect(typedMatches("", "")).toBe(false);
    expect(typedMatches("  ", "   ")).toBe(false);
  });
});
