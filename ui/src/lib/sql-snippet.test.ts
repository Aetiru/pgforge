import { describe, expect, it } from "vitest";
import { findSnippet, preview, wordBefore } from "./sql-snippet";
import type { Snippet } from "./ipc";

function snippet(abbreviation: string, body = "algo"): Snippet {
  return { id: abbreviation, abbreviation, body, description: "" };
}

describe("wordBefore", () => {
  it("toma la palabra que termina justo en el cursor", () => {
    const sql = "SELECT * FROM sf";
    expect(wordBefore(sql, sql.length)).toEqual({ from: 14, to: 16, word: "sf" });
  });

  it("no toma nada si el cursor no viene pegado a una palabra", () => {
    expect(wordBefore("SELECT sf ", 10)).toBeNull();
    expect(wordBefore("", 0)).toBeNull();
    expect(wordBefore("SELECT (", 8)).toBeNull();
  });

  it("corta a mitad de palabra en vez de tomarla entera", () => {
    // Con el cursor entre la `e` y la `l` de `select`, el tabulador no tiene por qué mirar `select`.
    expect(wordBefore("select", 3)?.word).toBe("sel");
  });

  it("una palabra puede llevar guion bajo, números y acentos", () => {
    const sql = "mi_tabla_2";
    expect(wordBefore(sql, sql.length)?.word).toBe("mi_tabla_2");
    expect(wordBefore("año", 3)?.word).toBe("año");
  });

  it("no cruza el punto de una calificación", () => {
    const sql = "public.sf";
    expect(wordBefore(sql, sql.length)).toEqual({ from: 7, to: 9, word: "sf" });
  });
});

describe("findSnippet", () => {
  const lista = [snippet("sf"), snippet("cte"), snippet("ij")];

  it("encuentra sin distinguir mayúsculas", () => {
    // Escrita en medio de una consulta en mayúsculas, `SF` es la misma que `sf`.
    expect(findSnippet(lista, "SF")?.abbreviation).toBe("sf");
    expect(findSnippet(lista, "sf")?.abbreviation).toBe("sf");
  });

  it("exige la palabra entera y no un prefijo", () => {
    expect(findSnippet(lista, "s")).toBeNull();
    expect(findSnippet(lista, "sfx")).toBeNull();
  });

  it("con la lista vacía no encuentra nada", () => {
    expect(findSnippet([], "sf")).toBeNull();
  });
});

describe("preview", () => {
  it("saca los huecos sin dejar el nombre del campo", () => {
    expect(preview("SELECT ${*}\nFROM ${tabla}\nWHERE ${}")).toBe("SELECT * FROM tabla WHERE");
  });

  it("de un hueco numerado no muestra el número", () => {
    expect(preview("SELECT ${1} FROM ${2:clientes}")).toBe("SELECT FROM clientes");
  });

  it("una llave escapada es una llave y no un hueco", () => {
    expect(preview("SELECT '\\{a\\}'")).toBe("SELECT '{a}'");
  });

  it("un parámetro de PostgreSQL no es un hueco", () => {
    // `$1` sin llaves es texto literal, que es lo que hace seguro usar la sintaxis de CodeMirror.
    expect(preview("SELECT * FROM t WHERE id = $1")).toBe("SELECT * FROM t WHERE id = $1");
  });
});
