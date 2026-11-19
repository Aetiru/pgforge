import { describe, expect, it } from "vitest";
import { applyLiterals, findLiterals } from "./snippet-literals";
import { variablesOf } from "./snippet-vars";

const texts = (sql: string) => findLiterals(sql).map((l) => l.text);
const names = (sql: string) => findLiterals(sql).map((l) => l.suggestedName);

describe("findLiterals", () => {
  it("ofrece cadenas y números con el nombre de la columna con la que se comparan", () => {
    const sql = "SELECT * FROM v WHERE estado = 'activo' AND total > 100";
    expect(texts(sql)).toEqual(["activo", "100"]);
    expect(names(sql)).toEqual(["estado", "total"]);
    expect(findLiterals(sql).map((l) => l.kind)).toEqual(["string", "number"]);
  });

  it("el rango de una cadena es el de adentro de las comillas", () => {
    const sql = "SELECT 'hola'";
    const [literal] = findLiterals(sql);
    expect(sql.slice(literal.from, literal.to)).toBe("hola");
  });

  it("una cadena con comillas duplicadas se lee entera", () => {
    expect(texts("WHERE nombre = 'O''Brien'")).toEqual(["O''Brien"]);
  });

  it("reconoce fechas y marcas de tiempo, por la forma y por el prefijo", () => {
    const sql =
      "WHERE a >= '2026-01-01' AND b < DATE '2026-02-01' AND c = TIMESTAMP '2026-03-01 10:00:00' AND d = '2026-03-01T10:00'";
    expect(findLiterals(sql).map((l) => l.kind)).toEqual(["date", "date", "timestamp", "timestamp"]);
  });

  it("junta los decimales en un solo número", () => {
    const [literal] = findLiterals("WHERE precio > 12.50");
    expect(literal.text).toBe("12.50");
    expect(literal.kind).toBe("number");
  });

  it("LIMIT y OFFSET se ofrecen con su nombre", () => {
    const sql = "SELECT * FROM t LIMIT 10 OFFSET 20";
    expect(names(sql)).toEqual(["limite", "desde"]);
    expect(names("SELECT * FROM t FETCH FIRST 5 ROWS ONLY")).toEqual(["limite"]);
  });

  it("la columna de un IN y de un BETWEEN, con nombres únicos", () => {
    expect(names("WHERE estado IN ('a', 'b')")).toEqual(["estado", "estado_2"]);
    expect(names("WHERE fecha BETWEEN '2026-01-01' AND '2026-02-01'")).toEqual(["fecha", "fecha_2"]);
  });

  it("lee la columna adentro de una función y detrás de un alias", () => {
    expect(names("WHERE lower(email) = 'a@b.c'")).toEqual(["email"]);
    expect(names("WHERE c.fecha_alta >= '2026-01-01'")).toEqual(["fecha_alta"]);
  });

  it("sin contexto usa valor1, valor2", () => {
    expect(names("SELECT 'a', 'b'")).toEqual(["valor1", "valor2"]);
    expect(names("INSERT INTO t VALUES (1, 'x')")).toEqual(["valor1", "valor2"]);
  });

  it("no ofrece 1 = 1, la posición de ORDER BY y GROUP BY, ni la medida de un tipo", () => {
    expect(texts("SELECT * FROM t WHERE 1 = 1")).toEqual([]);
    expect(texts("SELECT a, b FROM t GROUP BY 1, 2 ORDER BY 2")).toEqual([]);
    expect(texts("CREATE TABLE t (a varchar(20), b numeric(10,2))")).toEqual([]);
  });

  it("después de ORDER BY los números vuelven a ser datos", () => {
    expect(texts("SELECT * FROM t ORDER BY 1 LIMIT 5")).toEqual(["5"]);
    expect(texts("SELECT * FROM t ORDER BY a + 1")).toEqual(["1"]);
  });

  it("no ofrece parámetros ni lo que está en comentarios, identificadores o dólares", () => {
    expect(texts("SELECT * FROM t WHERE a = $1 -- 'x' 5")).toEqual([]);
    expect(texts('SELECT "a 5" FROM t')).toEqual([]);
    expect(texts("DO $$ BEGIN PERFORM 'x', 5; END $$")).toEqual([]);
  });

  it("no ofrece las cadenas con prefijo pegado", () => {
    expect(texts("SELECT E'a\\nb', B'101', X'ff'")).toEqual([]);
  });

  it("no ofrece lo que ya está adentro de una variable", () => {
    expect(texts("WHERE a = ${x:5} AND b = 7")).toEqual(["7"]);
  });

  it("un texto sin cerrar no ofrece nada", () => {
    expect(findLiterals("WHERE a = 'abc")).toEqual([]);
  });

  it("los números dentro de un identificador no cuentan", () => {
    expect(texts("SELECT t1.c2 FROM t1 WHERE c2 = 3")).toEqual(["3"]);
  });
});

describe("applyLiterals", () => {
  const choose = (sql: string, pick?: (text: string) => boolean) =>
    findLiterals(sql)
      .filter((l) => !pick || pick(l.text))
      .map((l) => ({ from: l.from, to: l.to, name: l.suggestedName, default: l.text }));

  it("reemplaza por ${nombre:valor} y deja las comillas afuera", () => {
    const sql = "WHERE estado = 'activo' AND total > 100";
    expect(applyLiterals(sql, choose(sql))).toBe(
      "WHERE estado = '${estado:activo}' AND total > ${total:100}",
    );
  });

  it("devuelve un cuerpo que variablesOf lee", () => {
    const sql = "WHERE estado = 'activo' LIMIT 10";
    const body = applyLiterals(sql, choose(sql));
    expect(variablesOf(body).map((v) => [v.name, v.default])).toEqual([
      ["estado", "activo"],
      ["limite", "10"],
    ]);
  });

  it("deja lo que no se eligió", () => {
    const sql = "WHERE a = 'x' AND b = 5";
    expect(applyLiterals(sql, choose(sql, (t) => t === "5"))).toBe("WHERE a = 'x' AND b = ${b:5}");
  });

  it("dos con el mismo nombre y el mismo valor comparten variable; con otro valor, nombre_2", () => {
    const sql = "SELECT 5, 5, 6";
    const chosen = findLiterals(sql).map((l) => ({ from: l.from, to: l.to, name: "n", default: l.text }));
    expect(applyLiterals(sql, chosen)).toBe("SELECT ${n:5}, ${n:5}, ${n_2:6}");
  });

  it("no toca un valor con llaves: no cabe como valor por omisión", () => {
    const sql = "SELECT '{a}', 5";
    expect(applyLiterals(sql, choose(sql))).toBe("SELECT '{a}', ${valor2:5}");
  });

  it("limpia los nombres inválidos y reemplaza el vacío", () => {
    const sql = "SELECT 5, 6";
    const [a, b] = findLiterals(sql);
    const out = applyLiterals(sql, [
      { from: a.from, to: a.to, name: "mi valor!", default: "5" },
      { from: b.from, to: b.to, name: "", default: "6" },
    ]);
    expect(out).toBe("SELECT ${mi_valor_:5}, ${valor:6}");
  });

  it("descarta los rangos que se pisan", () => {
    const out = applyLiterals("SELECT 12345", [
      { from: 7, to: 12, name: "a", default: "12345" },
      { from: 8, to: 10, name: "b", default: "23" },
    ]);
    expect(out).toBe("SELECT ${a:12345}");
  });
});
