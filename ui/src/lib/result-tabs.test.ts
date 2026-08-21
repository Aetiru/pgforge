import { describe, expect, it } from "vitest";
import { resultLabel } from "./result-tabs";
import type { ResultSet } from "./query.svelte";

function rows(overrides: Partial<Extract<ResultSet["outcome"], { kind: "rows" }>> = {}): ResultSet {
  return {
    index: 0,
    line: 1,
    offset: 0,
    outcome: {
      kind: "rows",
      columns: [],
      rows: [],
      rowCount: 0,
      truncated: false,
      seconds: 0,
      ...overrides,
    },
  };
}

function command(
  overrides: Partial<Extract<ResultSet["outcome"], { kind: "command" }>> = {},
): ResultSet {
  return {
    index: 0,
    line: 1,
    offset: 0,
    outcome: { kind: "command", tag: "UPDATE", affected: 0, seconds: 0, ...overrides },
  };
}

describe("resultLabel", () => {
  it("singular con una sola fila", () => {
    expect(resultLabel(rows({ rowCount: 1 })).detail).toBe("1 fila");
  });

  it("plural con varias filas", () => {
    expect(resultLabel(rows({ rowCount: 340 })).detail).toBe("340 filas");
  });

  it("marca el recorte cuando el techo de la página cortó el resultado", () => {
    // `count()` agrupa desde cinco cifras en español (10.000), no desde cuatro: no es un error de
    // redondeo, es la misma regla que ya prueba `format.test.ts`.
    expect(resultLabel(rows({ rowCount: 12000, truncated: true })).detail).toBe("12.000+ filas");
  });

  it("un comando sin filas muestra la etiqueta y cuántas tocó", () => {
    expect(resultLabel(command({ tag: "INSERT", affected: 12 })).detail).toBe("INSERT · 12");
  });

  it("el título numera desde 1 aunque el índice del script empiece en 0", () => {
    expect(resultLabel(rows()).title).toBe("#1");
    expect(resultLabel({ ...rows(), index: 3 }).title).toBe("#4");
  });

  it("el hint lleva la línea del script y no se confunde con el índice", () => {
    const label = resultLabel({ ...rows({ rowCount: 8 }), index: 1, line: 14 });
    expect(label.hint).toBe("Sentencia 2 — línea 14 — 8 filas");
  });
});
