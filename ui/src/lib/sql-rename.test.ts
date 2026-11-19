import { describe, expect, it } from "vitest";
import { aliasAt, renameAlias, validateAliasName } from "./sql-rename";

function at(marked: string) {
  const pos = marked.indexOf("|");
  return { text: marked.replace("|", ""), pos };
}

/** Aplica los cambios de atrás para adelante, como hace CodeMirror con una lista de cambios. */
function rename(marked: string, newName: string): string | null {
  const { text, pos } = at(marked);
  const found = aliasAt(text, pos);
  if (!found) return null;
  let result = text;
  for (const change of renameAlias(found.occurrences, newName).reverse()) {
    result = result.slice(0, change.from) + change.insert + result.slice(change.to);
  }
  return result;
}

describe("aliasAt", () => {
  it("encuentra la declaración y todos los usos de un alias", () => {
    const sql = "SELECT c.nombre, c.id FROM clientes c JOIN pedidos p ON p.cliente_id = c.id WHERE c.id > 1";
    expect(rename(`SELECT c.nombre, c.id FROM clientes |c JOIN pedidos p ON p.cliente_id = c.id WHERE c.id > 1`, "cli")).toBe(
      sql.replace(/\bc\b/g, "cli"),
    );
  });

  it("también desde un uso con punto", () => {
    expect(rename("SELECT |c.nombre FROM clientes c", "x")).toBe("SELECT x.nombre FROM clientes x");
  });

  it("alias con AS y sin AS, y en una lista separada por comas", () => {
    expect(rename("SELECT a.x, b.y FROM t1 AS |a, t2 b WHERE a.x = b.y", "uno")).toBe(
      "SELECT uno.x, b.y FROM t1 AS uno, t2 b WHERE uno.x = b.y",
    );
    expect(rename("SELECT a.x, b.y FROM t1 AS a, t2 |b WHERE a.x = b.y", "dos")).toBe(
      "SELECT a.x, dos.y FROM t1 AS a, t2 dos WHERE a.x = dos.y",
    );
  });

  it("alias de una subconsulta y de UPDATE / INSERT INTO … AS", () => {
    expect(rename("SELECT |s.n FROM (SELECT 1 AS n) AS s", "q")).toBe("SELECT q.n FROM (SELECT 1 AS n) AS q");
    expect(rename("UPDATE t |u SET a = 1 WHERE u.id = 3", "z")).toBe("UPDATE t z SET a = 1 WHERE z.id = 3");
    expect(rename("INSERT INTO t AS |x (a) VALUES (1) ON CONFLICT (a) DO UPDATE SET a = x.a + 1", "n")).toBe(
      "INSERT INTO t AS n (a) VALUES (1) ON CONFLICT (a) DO UPDATE SET a = n.a + 1",
    );
  });

  it("una CTE: su declaración, sus usos como tabla y sus calificadores", () => {
    expect(rename("WITH |v AS (SELECT 1 AS a) SELECT v.a FROM v JOIN v w ON true", "ventas")).toBe(
      "WITH ventas AS (SELECT 1 AS a) SELECT ventas.a FROM ventas JOIN ventas w ON true",
    );
    expect(aliasAt(...(Object.values(at("WITH |v AS (SELECT 1) SELECT 1 FROM v")) as [string, number]))?.kind).toBe("cte");
  });

  it("no toca cadenas, comentarios ni lo que viene después de un punto", () => {
    expect(rename("SELECT |c.x, 'c.x', s.c.x FROM t c -- c.x\n", "k")).toBe("SELECT k.x, 'c.x', s.c.x FROM t k -- c.x\n");
  });

  it("no es un alias: una tabla sin alias, una columna, una palabra clave", () => {
    expect(aliasAt(...(Object.values(at("SELECT clientes.nom|bre FROM clientes")) as [string, number]))).toBeNull();
    expect(aliasAt(...(Object.values(at("SELECT |* FROM clientes c")) as [string, number]))).toBeNull();
    expect(aliasAt(...(Object.values(at("SEL|ECT 1 FROM t c")) as [string, number]))).toBeNull();
  });

  it("un alias con comillas se encuentra por su nombre", () => {
    expect(rename('SELECT "Mi".x FROM t |"Mi"', "m")).toBe("SELECT m.x FROM t m");
  });

  it("solo mira la sentencia del cursor", () => {
    expect(rename("SELECT c.x FROM t c; SELECT |c.y FROM u c", "k")).toBe("SELECT c.x FROM t c; SELECT k.y FROM u k");
  });

  it("devuelve las apariciones en orden y el nombre tal como está escrito", () => {
    const { text, pos } = at("SELECT |c.x FROM t c WHERE c.y = 1");
    const found = aliasAt(text, pos)!;
    expect(found.name).toBe("c");
    expect(found.kind).toBe("alias");
    expect(found.occurrences.map((o) => o.from)).toEqual([...found.occurrences.map((o) => o.from)].sort((a, b) => a - b));
    expect(found.occurrences).toHaveLength(3);
  });
});

describe("validateAliasName", () => {
  const check = (marked: string, name: string) => {
    const { text, pos } = at(marked);
    return validateAliasName(text, pos, name);
  };
  const sql = "SELECT |c.x FROM clientes c JOIN pedidos p ON true";

  it("acepta un nombre libre", () => {
    expect(check(sql, "cli")).toBeNull();
    expect(check(sql, '"Mi Alias"')).toBeNull();
  });

  it("rechaza vacío, mal formado y palabras reservadas", () => {
    expect(check(sql, "  ")).toContain("Escribe un nombre");
    expect(check(sql, "1a")).toContain("empieza con una letra");
    expect(check(sql, "a b")).toContain("empieza con una letra");
    expect(check(sql, "order")).toContain("palabra reservada");
    expect(check(sql, '"abc')).toContain("comillas dobles");
  });

  it("rechaza un nombre que ya usa otra tabla o alias de la sentencia", () => {
    expect(check(sql, "p")).toContain("ya se usa");
    expect(check(sql, "P")).toContain("ya se usa");
    expect(check("SELECT x FROM t |c, u d WHERE d.a = c.b", "d")).toContain("ya se usa");
    expect(check("SELECT x FROM t |c, u WHERE u.a = c.b", "u")).toContain("ya se usa");
  });

  it("rechaza el mismo nombre y un cursor que no está sobre un alias", () => {
    expect(check(sql, "c")).toContain("no cambió");
    expect(check("SELECT |1 FROM t c", "z")).toContain("no está sobre un alias");
  });

  it("puede reutilizar el nombre de un alias de OTRA sentencia", () => {
    expect(check("SELECT |c.x FROM t c; SELECT 1 FROM u p", "p")).toBeNull();
  });
});
