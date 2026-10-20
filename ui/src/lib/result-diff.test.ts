import { describe, expect, it } from "vitest";
import { commonColumns, diffResults, diffSummary } from "./result-diff";

const a = {
  columns: ["id", "nombre"],
  rows: [
    ["1", "ana"],
    ["2", "beto"],
    ["3", null],
  ],
};

describe("diffResults por posición", () => {
  it("marca idénticos dos resultados iguales", () => {
    const d = diffResults(a, structuredClone(a), { key: null });
    expect(d.counts).toEqual({ same: 3, changed: 0, added: 0, removed: 0, cells: 0 });
    expect(diffSummary(d)).toContain("Idénticos");
  });

  it("marca la celda que cambió y solo esa", () => {
    const b = { columns: a.columns, rows: [["1", "ana"], ["2", "BETO"], ["3", null]] };
    const d = diffResults(a, b, { key: null });
    expect(d.rows[1].status).toBe("changed");
    expect(d.rows[1].changed).toEqual([false, true]);
    expect(d.counts.cells).toBe(1);
  });

  it("distingue NULL de la cadena vacía", () => {
    const b = { columns: a.columns, rows: [["1", "ana"], ["2", "beto"], ["3", ""]] };
    expect(diffResults(a, b, { key: null }).rows[2].changed).toEqual([false, true]);
  });

  it("las filas de más quedan como agregadas o quitadas", () => {
    const b = { columns: a.columns, rows: [["1", "ana"]] };
    expect(diffResults(a, b, { key: null }).counts.removed).toBe(2);
    expect(diffResults(b, a, { key: null }).counts.added).toBe(2);
  });

  it("compara solo las columnas comunes y informa el resto", () => {
    const b = { columns: ["nombre", "edad", "id"], rows: [["ana", "3", "1"]] };
    const d = diffResults(a, b, { key: null });
    expect(d.columns).toEqual(["id", "nombre"]);
    expect(d.onlyRight).toEqual(["edad"]);
    expect(d.rows[0].status).toBe("same");
  });
});

describe("diffResults por clave", () => {
  it("no se confunde cuando una fila se inserta arriba", () => {
    const b = { columns: a.columns, rows: [["0", "zoe"], ...a.rows] };
    const d = diffResults(a, b, { key: ["id"] });
    expect(d.counts).toMatchObject({ same: 3, changed: 0, added: 1, removed: 0 });
    // Por posición, en cambio, todo aparece distinto.
    expect(diffResults(a, b, { key: null }).counts.changed).toBeGreaterThan(0);
  });

  it("detecta cambiadas, agregadas y quitadas", () => {
    const b = { columns: a.columns, rows: [["2", "bruno"], ["3", null], ["9", "nuevo"]] };
    const d = diffResults(a, b, { key: ["id"] });
    expect(d.rows.map((r) => [r.key, r.status])).toEqual([
      ["1", "removed"],
      ["2", "changed"],
      ["3", "same"],
      ["9", "added"],
    ]);
  });

  it("empareja claves repetidas por orden de aparición", () => {
    const x = { columns: ["k", "v"], rows: [["1", "a"], ["1", "b"]] };
    const y = { columns: ["k", "v"], rows: [["1", "a"], ["1", "c"]] };
    expect(diffResults(x, y, { key: ["k"] }).rows.map((r) => r.status)).toEqual(["same", "changed"]);
  });

  it("acepta una clave compuesta", () => {
    const x = { columns: ["a", "b", "v"], rows: [["1", "x", "p"], ["1", "y", "q"]] };
    const y = { columns: ["a", "b", "v"], rows: [["1", "y", "q"], ["1", "x", "z"]] };
    expect(diffResults(x, y, { key: ["a", "b"] }).rows.map((r) => r.status)).toEqual(["changed", "same"]);
  });

  it("rechaza una clave que no está en los dos lados", () => {
    expect(() => diffResults(a, a, { key: ["nope"] })).toThrow();
    expect(() => diffResults(a, a, { key: [] })).toThrow();
  });
});

describe("commonColumns", () => {
  it("devuelve las comunes en el orden del izquierdo, sin repetir", () => {
    expect(
      commonColumns({ columns: ["x", "y", "x"], rows: [] }, { columns: ["y", "x"], rows: [] }),
    ).toEqual(["x", "y"]);
  });
});
