import { describe, expect, it } from "vitest";
import { CAP, countQuery } from "./affected-count";

const wrap = (inner: string) => `SELECT count(*) FROM (SELECT 1 FROM ${inner} LIMIT ${CAP + 1}) AS _n`;

describe("countQuery", () => {
  it("UPDATE con WHERE", () => {
    const plan = countQuery("UPDATE pedidos SET estado = 'baja' WHERE creado < '2020-01-01';");
    expect(plan).toEqual({ kind: "update", hasWhere: true, sql: wrap("pedidos WHERE creado < '2020-01-01'") });
  });

  it("DELETE con alias y WHERE", () => {
    expect(countQuery("DELETE FROM public.pedidos AS p WHERE p.id > 10")?.sql).toBe(
      wrap("public.pedidos AS p WHERE p.id > 10"),
    );
  });

  it("sin WHERE avisa que toca toda la tabla", () => {
    expect(countQuery("DELETE FROM pedidos")).toMatchObject({ hasWhere: false, sql: wrap("pedidos") });
    expect(countQuery("UPDATE pedidos SET a = 1")).toMatchObject({ hasWhere: false });
  });

  it("no confunde WHERE con un alias", () => {
    expect(countQuery("DELETE FROM pedidos WHERE x = 1")?.sql).toBe(wrap("pedidos WHERE x = 1"));
  });

  it("corta en RETURNING", () => {
    expect(countQuery("UPDATE t SET a = 1 WHERE b = 2 RETURNING *")?.sql).toBe(wrap("t WHERE b = 2"));
  });

  it("UPDATE ... FROM y DELETE ... USING suman la otra tabla", () => {
    expect(countQuery("UPDATE a SET x = b.x FROM b WHERE a.id = b.id")?.sql).toBe(wrap("a, b WHERE a.id = b.id"));
    expect(countQuery("DELETE FROM a USING b WHERE a.id = b.id")?.sql).toBe(wrap("a, b WHERE a.id = b.id"));
  });

  it("un FROM o WHERE adentro de un paréntesis no corta", () => {
    expect(countQuery("UPDATE t SET a = (SELECT max(x) FROM u WHERE u.k = 1) WHERE t.id = 3")?.sql).toBe(
      wrap("t WHERE t.id = 3"),
    );
  });

  it("los comentarios y las cadenas no engañan", () => {
    expect(countQuery("UPDATE t SET a = 'WHERE' -- WHERE x\n WHERE b = 1")?.sql).toBe(wrap("t WHERE b = 1"));
  });

  it("lo que no entiende devuelve null", () => {
    expect(countQuery("SELECT 1")).toBeNull();
    expect(countQuery("WITH x AS (SELECT 1) DELETE FROM t")).toBeNull();
    expect(countQuery("DELETE FROM a; DELETE FROM b")).toBeNull();
    expect(countQuery("")).toBeNull();
  });
});
