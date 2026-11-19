import { describe, expect, it } from "vitest";
import { activeParameter, callAt } from "./sql-call";

/** Dónde está el cursor se marca con `|`. */
function at(marked: string) {
  const pos = marked.indexOf("|");
  return callAt(marked.replace("|", ""), pos);
}

describe("callAt", () => {
  it("reconoce la llamada y el argumento en el que se está", () => {
    expect(at("SELECT date_trunc(|")).toMatchObject({ name: "date_trunc", argIndex: 0 });
    expect(at("SELECT date_trunc('day', |")).toMatchObject({ name: "date_trunc", argIndex: 1 });
    expect(at("SELECT substr(a, 2, |3)")).toMatchObject({ name: "substr", argIndex: 2 });
  });

  it("devuelve dónde está el nombre, para el globo", () => {
    const sql = "SELECT now(|)";
    const call = callAt(sql.replace("|", ""), sql.indexOf("|"));
    expect(sql.slice(call!.nameFrom, call!.nameTo)).toBe("now");
  });

  it("toma la llamada más interna e ignora las comas de los paréntesis anidados", () => {
    expect(at("SELECT round(sum(a, b), |)")).toMatchObject({ name: "round", argIndex: 1 });
    expect(at("SELECT round(sum(a, |b), 2)")).toMatchObject({ name: "sum", argIndex: 1 });
    expect(at("SELECT f((1, 2), |)")).toMatchObject({ name: "f", argIndex: 1 });
  });

  it("no cuenta comas de cadenas ni de comentarios", () => {
    expect(at("SELECT concat('a,b,c', |)")).toMatchObject({ name: "concat", argIndex: 1 });
    expect(at("SELECT f(a /* , */, |)")).toMatchObject({ name: "f", argIndex: 1 });
  });

  it("entiende esquema.función", () => {
    expect(at("SELECT public.mi_fn(|)")).toMatchObject({ schema: "public", name: "mi_fn" });
  });

  it("no es una llamada IN, VALUES, EXISTS, AS, ON ni un tipo con medida", () => {
    expect(at("SELECT 1 WHERE a IN (|1, 2)")).toBeNull();
    expect(at("INSERT INTO t VALUES (|1)")).toBeNull();
    expect(at("SELECT 1 WHERE EXISTS (|SELECT 1)")).toBeNull();
    expect(at("WITH x AS (|SELECT 1) SELECT 1")).toBeNull();
    expect(at("SELECT x::numeric(10, |2)")).toBeNull();
    expect(at("SELECT 1 FROM a JOIN b ON (|a.x = b.x)")).toBeNull();
  });

  it("no es una llamada el INSERT INTO t ( ni el CREATE TABLE t (", () => {
    expect(at("INSERT INTO t (a, |b)")).toBeNull();
    expect(at("CREATE TABLE t (a int, |b int)")).toBeNull();
    expect(at("CREATE INDEX i ON t (|a)")).toBeNull();
  });

  it("sí lo es una función del FROM o del ON de un JOIN", () => {
    expect(at("SELECT * FROM generate_series(1, |3)")).toMatchObject({ name: "generate_series", argIndex: 1 });
    expect(at("SELECT 1 FROM a JOIN b ON lower(|a.x) = b.y")).toMatchObject({ name: "lower" });
  });

  it("una llamada ya cerrada, un cursor afuera o la sentencia anterior no cuentan", () => {
    expect(at("SELECT f(1)|")).toBeNull();
    expect(at("SELECT f(1); SELECT |")).toBeNull();
    expect(at("SELECT |")).toBeNull();
  });

  it("un cursor sobre una llamada anidada en otra función devuelve la de afuera al salir", () => {
    expect(at("SELECT f(g(1), |)")).toMatchObject({ name: "f", argIndex: 1 });
  });
});

describe("activeParameter", () => {
  it("parte los parámetros y marca el activo", () => {
    const r = activeParameter("field text, source timestamp with time zone", 1);
    expect(r.parts).toEqual(["field text", "source timestamp with time zone"]);
    expect(r.activeIndex).toBe(1);
  });

  it("respeta las comas de un tipo con medida y de un valor por omisión", () => {
    expect(activeParameter("a numeric(10,2), b text DEFAULT 'x,y'", 0).parts).toEqual([
      "a numeric(10,2)",
      "b text DEFAULT 'x,y'",
    ]);
  });

  it("acota el índice al último y, con VARIADIC, lo fija ahí", () => {
    expect(activeParameter("a integer, b integer", 7).activeIndex).toBe(1);
    expect(activeParameter("a text, VARIADIC xs integer[] DEFAULT 1", 5).activeIndex).toBe(1);
    expect(activeParameter("a text, VARIADIC xs integer[]", 0).activeIndex).toBe(0);
  });

  it("sin parámetros devuelve una lista vacía", () => {
    expect(activeParameter("", 0)).toEqual({ parts: [], activeIndex: 0 });
  });
});
