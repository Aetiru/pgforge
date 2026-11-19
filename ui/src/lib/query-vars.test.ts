import { describe, expect, it } from "vitest";
import { fillVars, findVars, parseRemembered, unmapper, varNames } from "./query-vars";

describe("variables de consulta", () => {
  it("encuentra {{nombre}} con espacios opcionales", () => {
    const uses = findVars("SELECT {{a}}, {{ b }}");
    expect(uses.map((u) => u.name)).toEqual(["a", "b"]);
  });

  it("no pregunta las comentadas", () => {
    expect(findVars("SELECT 1 -- {{a}}\n, {{b}} /* {{c}} /* {{d}} */ {{e}} */ , {{f}}").map((u) => u.name)).toEqual([
      "b",
      "f",
    ]);
  });

  it("vale adentro de una cadena y un -- adentro de una cadena no comenta", () => {
    expect(findVars("SELECT '-- {{a}}'").map((u) => u.name)).toEqual(["a"]);
  });

  it("no toma arreglos ni llaves que no son variables", () => {
    expect(findVars("SELECT '{{1,2},{3,4}}'::int[]")).toEqual([]);
    expect(findVars("SELECT '{{}}'")).toEqual([]);
  });

  it("pregunta una vez por nombre y reemplaza todas", () => {
    const sql = "SELECT {{x}} + {{x}}";
    const uses = findVars(sql);
    expect(varNames(uses)).toEqual(["x"]);
    expect(fillVars(sql, uses, { x: "7" })).toBe("SELECT 7 + 7");
  });

  it("reemplaza tal cual, sin escapar", () => {
    const sql = "WHERE n = '{{n}}'";
    expect(fillVars(sql, findVars(sql), { n: "a" })).toBe("WHERE n = 'a'");
  });

  it("devuelve la posición de un error al texto escrito", () => {
    const sql = "SELECT {{a}}, x FROM t";
    const uses = findVars(sql);
    const values = { a: "'un valor largo'" };
    const filled = fillVars(sql, uses, values);
    const unmap = unmapper(uses, values);
    expect(unmap(filled.indexOf("x FROM"))).toBe(sql.indexOf("x FROM"));
    expect(unmap(filled.indexOf("un valor"))).toBe(sql.indexOf("{{a}}"));
    expect(unmap(3)).toBe(3);
  });

  it("sanea lo recordado", () => {
    expect(parseRemembered('{"a":"1","b":2}')).toEqual({ a: "1" });
    expect(parseRemembered("basura")).toEqual({});
    expect(parseRemembered(null)).toEqual({});
  });
});
