import { describe, expect, it } from "vitest";

import { lastVacuumSeconds, tableHints } from "./table-health";
import type { TableStat } from "./ipc";

function stat(over: Partial<TableStat>): TableStat {
  return {
    schema: "public",
    table: "pedidos",
    liveTuples: 50_000,
    deadTuples: 10,
    deadRatio: 0.0002,
    totalBytes: 1,
    tableBytes: 1,
    indexBytes: 0,
    sequentialScans: 5,
    indexScans: 500,
    lastVacuumSeconds: null,
    lastAutovacuumSeconds: 60,
    lastAnalyzeSeconds: 60,
    ...over,
  };
}

describe("tableHints", () => {
  it("una tabla sana no tiene nada que decir", () => {
    expect(tableHints(stat({}))).toEqual([]);
  });

  it("avisa del vacuum solo cuando son muchas muertas y una parte grande", () => {
    expect(tableHints(stat({ deadTuples: 20_000, deadRatio: 0.3 }))[0].text).toContain("VACUUM");
    // Mucha proporción pero pocas filas: no vale un aviso.
    expect(tableHints(stat({ deadTuples: 20, deadRatio: 0.5 }))).toEqual([]);
    // Muchas muertas pero una parte chica del total.
    expect(tableHints(stat({ deadTuples: 5000, deadRatio: 0.02 }))).toEqual([]);
  });

  it("avisa de un ANALYZE que falta, salvo en una tabla vacía", () => {
    expect(tableHints(stat({ lastAnalyzeSeconds: null }))[0].text).toContain("ANALYZE");
    expect(tableHints(stat({ lastAnalyzeSeconds: null, liveTuples: 0 }))).toEqual([]);
  });

  it("sospecha de un índice que falta solo en una tabla grande leída casi siempre en secuencial", () => {
    const hints = tableHints(stat({ sequentialScans: 900, indexScans: 10 }));
    expect(hints.map((h) => h.level)).toEqual(["info"]);
    expect(tableHints(stat({ sequentialScans: 900, indexScans: 10, liveTuples: 500 }))).toEqual([]);
  });

  it("sin índices en la tabla, las lecturas secuenciales se comparan contra uno", () => {
    expect(tableHints(stat({ sequentialScans: 50, indexScans: null }))).toHaveLength(1);
  });
});

describe("lastVacuumSeconds", () => {
  it("toma el más reciente de los dos", () => {
    expect(lastVacuumSeconds(stat({ lastVacuumSeconds: 500, lastAutovacuumSeconds: 60 }))).toBe(60);
    expect(lastVacuumSeconds(stat({ lastVacuumSeconds: null, lastAutovacuumSeconds: null }))).toBeNull();
  });
});
