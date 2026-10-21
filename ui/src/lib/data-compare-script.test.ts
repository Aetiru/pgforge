import { describe, expect, it } from "vitest";
import { DEFAULT_RISKS } from "./compare-script";
import {
  cellText,
  countDataRisks,
  dataScriptOf,
  filterDataStatements,
  isChanged,
  keyLabel,
  summary,
} from "./data-compare-script";
import type { DataRowDiff, DataStatement } from "./ipc";

const statements: DataStatement[] = [
  { action: "insert", key: ["1"], risk: "safe", sql: "INSERT INTO t (id) VALUES ('1');" },
  { action: "update", key: ["2"], risk: "review", sql: "UPDATE t SET v = 'x' WHERE id = '2';" },
  { action: "delete", key: ["3"], risk: "destructive", sql: "DELETE FROM t WHERE id = '3';" },
];

describe("script de comparación de datos", () => {
  it("deja afuera lo destructivo por omisión", () => {
    const kept = filterDataStatements(statements, DEFAULT_RISKS);
    expect(kept.map((s) => s.action)).toEqual(["insert", "update"]);
  });

  it("incluye los DELETE cuando se tilda lo destructivo", () => {
    const kept = filterDataStatements(statements, { ...DEFAULT_RISKS, destructive: true });
    expect(dataScriptOf(kept)).toBe(statements.map((s) => s.sql).join("\n"));
  });

  it("un script vacío es texto vacío", () => {
    expect(dataScriptOf([])).toBe("");
  });

  it("cuenta las sentencias por riesgo", () => {
    expect(countDataRisks(statements)).toEqual({ safe: 1, review: 1, destructive: 1 });
  });
});

describe("filas del informe", () => {
  const different: DataRowDiff = {
    kind: "different",
    key: ["1", "a"],
    source: ["x", null],
    target: ["y", ""],
    changed: [0, 1],
  };

  it("NULL y cadena vacía se dibujan distinto", () => {
    expect(cellText(null)).toEqual({ text: "NULL", isNull: true });
    expect(cellText("")).toEqual({ text: "", isNull: false });
    expect(cellText("NULL")).toEqual({ text: "NULL", isNull: false });
  });

  it("marca como cambiadas solo las columnas de una fila distinta", () => {
    expect(isChanged(different, 1)).toBe(true);
    expect(isChanged({ ...different, changed: [0] }, 1)).toBe(false);
    expect(isChanged({ kind: "onlySource", key: ["1"], values: [] }, 0)).toBe(false);
  });

  it("escribe la clave compuesta en una línea", () => {
    expect(keyLabel(["1", "a"])).toBe("1 · a");
  });

  it("el resumen omite las clases vacías pero siempre dice cuántas son iguales", () => {
    expect(summary({ onlySource: 0, onlyTarget: 2, different: 0, equal: 5 })).toBe(
      "2 solo en el destino · 5 iguales",
    );
    expect(summary({ onlySource: 0, onlyTarget: 0, different: 0, equal: 0 })).toBe("0 iguales");
  });
});
