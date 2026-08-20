import { describe, expect, it } from "vitest";
import {
  countEntries,
  filterStatements,
  scriptOf,
  DEFAULT_RISKS,
  type RiskFilter,
} from "./compare-script";
import type { DiffEntry, SyncStatement } from "./ipc";

function statement(partial: Partial<SyncStatement>): SyncStatement {
  return {
    object: "table",
    name: "clientes",
    action: "alter",
    risk: "safe",
    sql: "SELECT 1;",
    ...partial,
  };
}

function entry(status: DiffEntry["status"], name: string): DiffEntry {
  return { kind: "table", name, status, details: [] };
}

describe("filtro por riesgo", () => {
  const statements = [
    statement({ risk: "safe", sql: "a;" }),
    statement({ risk: "review", sql: "b;" }),
    statement({ risk: "destructive", sql: "c;" }),
  ];

  it("deja fuera lo destructivo por omisión", () => {
    expect(filterStatements(statements, DEFAULT_RISKS).map((s) => s.sql)).toEqual(["a;", "b;"]);
  });

  it("con todo encendido no saca nada", () => {
    const all: RiskFilter = { safe: true, review: true, destructive: true };
    expect(filterStatements(statements, all)).toHaveLength(3);
  });

  it("puede quedarse solo con lo seguro", () => {
    const only: RiskFilter = { safe: true, review: false, destructive: false };
    expect(filterStatements(statements, only).map((s) => s.sql)).toEqual(["a;"]);
  });
});

describe("armado del script", () => {
  it("escribe el aviso como comentario arriba de su sentencia", () => {
    expect(scriptOf([statement({ sql: "DROP TABLE viejo;", note: "no existe en el origen" })])).toBe(
      "-- no existe en el origen\nDROP TABLE viejo;",
    );
  });

  it("separa las sentencias con una línea en blanco", () => {
    expect(scriptOf([statement({ sql: "a;" }), statement({ sql: "b;" })])).toBe("a;\n\nb;");
  });

  it("sin sentencias devuelve texto vacío", () => {
    expect(scriptOf([])).toBe("");
  });
});

describe("resumen del informe", () => {
  it("cuenta por estado", () => {
    const counts = countEntries([
      entry("onlySource", "a"),
      entry("onlySource", "b"),
      entry("different", "c"),
    ]);
    expect(counts).toEqual({ onlySource: 2, onlyTarget: 0, different: 1 });
  });
});
