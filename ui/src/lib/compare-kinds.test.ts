import { describe, expect, it } from "vitest";
import {
  categoryOf,
  compareKindLabel,
  compareKindLook,
  countCategories,
  filterByCategory,
  COMPARE_CATEGORIES,
} from "./compare-kinds";
import { countEntries, countRisks, filterStatements, scriptOf, DEFAULT_RISKS } from "./compare-script";
import type { CompareKind, DiffEntry, SyncStatement } from "./ipc";

// Si el núcleo agrega una clase y acá se olvidan, `categoryOf` la tiraría a «estructura» sin
// avisar: este registro obliga a decidirlo (el compilador marca la clave que falta).
const KINDS: Record<CompareKind, true> = {
  table: true,
  partitionedTable: true,
  foreignTable: true,
  view: true,
  materializedView: true,
  sequence: true,
  enum: true,
  composite: true,
  domain: true,
  range: true,
  function: true,
  procedure: true,
  trigger: true,
  policy: true,
  rowSecurity: true,
  grant: true,
};
const ALL = Object.keys(KINDS) as CompareKind[];

function entry(kind: CompareKind, status: DiffEntry["status"] = "different"): DiffEntry {
  return { kind, name: `${kind}-x`, status, details: [] };
}

describe("categorías del informe", () => {
  it("cada clase cae en una categoría que existe", () => {
    const known = COMPARE_CATEGORIES.map((c) => c.key);
    for (const kind of ALL) expect(known).toContain(categoryOf(kind));
  });

  it("separa estructura, lógica y acceso", () => {
    expect(categoryOf("table")).toBe("structure");
    expect(categoryOf("enum")).toBe("structure");
    expect(categoryOf("function")).toBe("logic");
    expect(categoryOf("procedure")).toBe("logic");
    expect(categoryOf("trigger")).toBe("logic");
    expect(categoryOf("policy")).toBe("access");
    expect(categoryOf("rowSecurity")).toBe("access");
    expect(categoryOf("grant")).toBe("access");
  });

  it("cuenta por categoría", () => {
    const entries = [entry("table"), entry("view"), entry("function"), entry("grant")];
    expect(countCategories(entries)).toEqual({ structure: 2, logic: 1, access: 1 });
  });

  it("apagar una categoría saca sus entradas y deja las demás", () => {
    const entries = [entry("table"), entry("function"), entry("policy")];
    const shown = { structure: true, logic: false, access: true };
    expect(filterByCategory(entries, shown).map((e) => e.kind)).toEqual(["table", "policy"]);
  });
});

describe("ícono y nombre", () => {
  it("toda clase tiene un nombre que no es su identificador crudo", () => {
    for (const kind of ALL) {
      const label = compareKindLabel(kind);
      expect(label, kind).not.toBe(kind);
      expect(label.length, kind).toBeGreaterThan(0);
    }
  });

  it("toda clase tiene ícono y color", () => {
    for (const kind of ALL) {
      const look = compareKindLook(kind);
      expect(look.icon, kind).toBeTruthy();
      expect(look.tone, kind).toBeTruthy();
    }
  });

  it("los permisos y la seguridad por filas llevan el color del acceso", () => {
    expect(compareKindLook("grant").tone).toBe(compareKindLook("policy").tone);
    expect(compareKindLook("rowSecurity").tone).toBe(compareKindLook("policy").tone);
  });
});

describe("el filtro por riesgo con las clases nuevas", () => {
  const statements: SyncStatement[] = [
    { object: "function", name: "f()", action: "create", risk: "safe", sql: "CREATE FUNCTION f();" },
    { object: "trigger", name: "t.x", action: "alter", risk: "review", sql: "CREATE TRIGGER x;" },
    { object: "grant", name: "t", action: "alter", risk: "review", sql: "GRANT SELECT ON t TO a;" },
    { object: "function", name: "g()", action: "drop", risk: "destructive", sql: "DROP FUNCTION g();" },
  ];

  it("deja fuera lo destructivo por omisión, sea de la clase que sea", () => {
    const kept = filterStatements(statements, DEFAULT_RISKS);
    expect(kept.map((s) => s.sql)).not.toContain("DROP FUNCTION g();");
    expect(kept).toHaveLength(3);
  });

  it("cuenta los riesgos y arma el script con las notas", () => {
    expect(countRisks(statements)).toEqual({ safe: 1, review: 2, destructive: 1 });
    expect(scriptOf([{ ...statements[2], note: "falla si el rol no existe" }])).toBe(
      "-- falla si el rol no existe\nGRANT SELECT ON t TO a;",
    );
  });

  it("los conteos por estado no distinguen la clase", () => {
    const counts = countEntries([
      entry("function", "onlySource"),
      entry("grant", "different"),
      entry("table", "onlyTarget"),
    ]);
    expect(counts).toEqual({ onlySource: 1, onlyTarget: 1, different: 1 });
  });
});
