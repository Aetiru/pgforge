import { describe, expect, it } from "vitest";
import { clauseAt, columnCompletion, cteRelations, selectAliases } from "./sql-complete";
import type { SchemaRelation } from "./ipc";

let oid = 0;
function relation(schema: string, name: string, columns: string[]): SchemaRelation {
  oid += 1;
  return { oid, schema, name, columns: columns.map((c) => ({ name: c, typeName: "text" })) };
}

const RELATIONS = [
  relation("public", "clientes", ["id", "nombre", "creado"]),
  relation("public", "pedidos", ["id", "cliente_id", "total"]),
];

/** Dónde está el cursor se marca con `|`. */
function at(marked: string) {
  return { sql: marked.replace("|", ""), cursor: marked.indexOf("|") };
}

describe("clauseAt", () => {
  const clause = (marked: string) => {
    const { sql, cursor } = at(marked);
    return clauseAt(sql, cursor);
  };

  it("reconoce cada cláusula", () => {
    expect(clause("SELECT |")).toBe("select");
    expect(clause("SELECT a FROM |")).toBe("from");
    expect(clause("SELECT a FROM t WHERE |")).toBe("where");
    expect(clause("SELECT a FROM t GROUP BY |")).toBe("group");
    expect(clause("SELECT a FROM t ORDER BY |")).toBe("order");
    expect(clause("SELECT a FROM t GROUP BY a HAVING |")).toBe("having");
    expect(clause("UPDATE t SET |")).toBe("set");
    expect(clause("INSERT INTO t VALUES (|")).toBe("values");
  });

  it("distingue la lista de columnas de un INSERT y el ON CONFLICT", () => {
    expect(clause("INSERT INTO t (a, |")).toBe("insert-columns");
    expect(clause("INSERT INTO public.t (|")).toBe("insert-columns");
    expect(clause("INSERT INTO t (a) VALUES (1) ON CONFLICT (|")).toBe("on-conflict");
  });

  it("una subconsulta tiene su propia cláusula y al cerrarla se vuelve a la de afuera", () => {
    expect(clause("SELECT a FROM t WHERE id IN (SELECT |")).toBe("select");
    expect(clause("SELECT a FROM t WHERE id IN (SELECT 1) AND |")).toBe("where");
  });

  it("una llamada dentro de una cláusula sigue en esa cláusula", () => {
    expect(clause("SELECT a FROM t ORDER BY lower(|")).toBe("order");
  });

  it("no se confunde con la sentencia anterior ni con la palabra que se está tecleando", () => {
    expect(clause("SELECT a FROM t WHERE x = 1; SELECT |")).toBe("select");
    expect(clause("SELECT a FROM t ORDER BY ve|")).toBe("order");
    expect(clause("SELECT 'FROM;' || |")).toBe("select");
  });

  it("sin cláusula es «other»", () => {
    expect(clause("|")).toBe("other");
    expect(clause("WITH x AS (SELECT 1) |")).toBe("other");
  });
});

describe("selectAliases", () => {
  const aliases = (marked: string) => {
    const { sql, cursor } = at(marked);
    return selectAliases(sql, cursor);
  };

  it("lee los alias con AS y los escritos sin AS", () => {
    expect(aliases("SELECT sum(total) AS ventas, count(*) n, nombre FROM t ORDER BY |")).toEqual(["ventas", "n"]);
  });

  it("no toma como alias una columna calificada, un cast ni una palabra de cierre", () => {
    expect(aliases("SELECT a.nombre, x::int, CASE WHEN a THEN 1 END, 1 + 2 FROM t ORDER BY |")).toEqual([]);
  });

  it("solo mira el SELECT del nivel del cursor", () => {
    expect(aliases("SELECT (SELECT 1 AS interno) AS externo FROM t ORDER BY |")).toEqual(["externo"]);
  });

  it("sin SELECT no hay alias", () => {
    expect(aliases("UPDATE t SET |")).toEqual([]);
  });

  it("DISTINCT ON no cuenta como columna", () => {
    expect(aliases("SELECT DISTINCT ON (a) b AS beta FROM t ORDER BY |")).toEqual(["beta"]);
  });
});

describe("cteRelations", () => {
  const ctes = (marked: string) => {
    const { sql, cursor } = at(marked);
    return cteRelations(sql, cursor, RELATIONS).map((r) => [r.name, r.columns.map((c) => c.name)]);
  };

  it("saca las columnas de la lista SELECT, con alias y último nombre", () => {
    expect(ctes("WITH ventas AS (SELECT c.nombre, sum(total) AS monto, count(*) FROM pedidos p) SELECT | FROM ventas")).toEqual([
      ["ventas", ["nombre", "monto", "count"]],
    ]);
  });

  it("resuelve * con las tablas del catálogo", () => {
    expect(ctes("WITH x AS (SELECT * FROM clientes) SELECT | FROM x")).toEqual([["x", ["id", "nombre", "creado"]]]);
    expect(ctes("WITH x AS (SELECT p.* FROM pedidos p JOIN clientes c ON true) SELECT | FROM x")).toEqual([
      ["x", ["id", "cliente_id", "total"]],
    ]);
  });

  it("las columnas que nombra el propio WITH mandan", () => {
    expect(ctes("WITH x (a, b) AS (SELECT id, nombre FROM clientes) SELECT | FROM x")).toEqual([["x", ["a", "b"]]]);
  });

  it("varias CTE, y una puede usar a la anterior con *", () => {
    expect(
      ctes("WITH a AS (SELECT id FROM clientes), b AS (SELECT * FROM a) SELECT | FROM b"),
    ).toEqual([
      ["a", ["id"]],
      ["b", ["id"]],
    ]);
  });

  it("entiende NOT MATERIALIZED y RECURSIVE", () => {
    expect(ctes("WITH RECURSIVE x AS NOT MATERIALIZED (SELECT 1 AS uno) SELECT | FROM x")).toEqual([["x", ["uno"]]]);
  });

  it("con el cuerpo sin cerrar —se está escribiendo adentro— no falla", () => {
    expect(() => ctes("WITH x AS (SELECT id, | FROM clientes")).not.toThrow();
  });

  it("sin WITH no hay CTE, y los oid son negativos para que no se confundan con tablas", () => {
    expect(ctes("SELECT | FROM clientes")).toEqual([]);
    const { sql, cursor } = at("WITH x AS (SELECT 1 AS a) SELECT |");
    expect(cteRelations(sql, cursor, RELATIONS)[0].oid).toBeLessThan(0);
  });
});

describe("columnCompletion", () => {
  /** Lo mínimo de `CompletionContext` que usa la fuente: el texto, el cursor y `matchBefore`. */
  function complete(marked: string, rels: SchemaRelation[] = RELATIONS, explicit = false) {
    const { sql, cursor } = at(marked);
    const context = {
      pos: cursor,
      explicit,
      state: { doc: { toString: () => sql } },
      matchBefore(expr: RegExp) {
        const lineStart = sql.lastIndexOf("\n", cursor - 1) + 1;
        const before = sql.slice(lineStart, cursor);
        const match = new RegExp(`(?:${expr.source})$`, expr.flags).exec(before);
        return match ? { from: lineStart + match.index, to: cursor, text: match[0] } : null;
      },
    };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return columnCompletion(() => rels)(context as any);
  }
  const labels = (marked: string, rels?: SchemaRelation[]) =>
    complete(marked, rels, true)?.options.map((o) => o.label) ?? null;

  it("en el INSERT ofrece solo las columnas de la tabla que faltan escribir", () => {
    expect(labels("INSERT INTO clientes (id, |")).toEqual(["nombre", "creado"]);
    expect(labels("INSERT INTO clientes (|")).toEqual(["id", "nombre", "creado"]);
  });

  it("en ON CONFLICT ofrece todas las columnas de la tabla", () => {
    expect(labels("INSERT INTO clientes (id) VALUES (1) ON CONFLICT (|")).toEqual(["id", "nombre", "creado"]);
  });

  it("EXCLUDED. ofrece las columnas de la tabla del INSERT", () => {
    const result = complete("INSERT INTO clientes (id) VALUES (1) ON CONFLICT (id) DO UPDATE SET nombre = excluded.no|");
    expect(result?.options.map((o) => o.label)).toEqual(["id", "nombre", "creado"]);
    expect(result?.options[0].detail).toBe("excluded");
    const sql = "INSERT INTO clientes (id) VALUES (1) ON CONFLICT (id) DO UPDATE SET nombre = excluded.no";
    expect(sql.slice(result!.from)).toBe("no");
  });

  it("EXCLUDED. sin INSERT no ofrece nada", () => {
    expect(labels("SELECT excluded.|")).toBeNull();
  });

  it("ORDER BY suma los alias del SELECT a las columnas", () => {
    const result = complete("SELECT sum(total) AS ventas FROM pedidos ORDER BY |", RELATIONS, true);
    const options = result!.options;
    expect(options.map((o) => o.label)).toContain("ventas");
    expect(options.find((o) => o.label === "ventas")?.type).toBe("variable");
    expect(options.map((o) => o.label)).toContain("total");
  });

  it("los alias no se ofrecen fuera de ORDER BY, GROUP BY y HAVING", () => {
    expect(labels("SELECT sum(total) AS ventas FROM pedidos WHERE |")).not.toContain("ventas");
  });

  it("las columnas de una CTE se ofrecen cuando está en el FROM", () => {
    expect(labels("WITH v AS (SELECT total AS monto FROM pedidos) SELECT | FROM v")).toContain("monto");
  });

  it("el nombre de una CTE se ofrece donde va una tabla", () => {
    const result = complete("WITH v AS (SELECT 1 AS a) SELECT 1 FROM |", RELATIONS, true);
    expect(result?.options.find((o) => o.label === "v")?.detail).toBe("CTE");
  });

  it("detrás de un punto no ofrece nada (contesta el dialecto)", () => {
    expect(labels("SELECT c.| FROM clientes c")).toBeNull();
  });
});
