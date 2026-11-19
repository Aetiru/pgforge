import { describe, expect, it } from "vitest";
import {
  askedVariables,
  duplicateOf,
  findHoles,
  formToScope,
  freeCopyName,
  hereOnly,
  importSummary,
  parseDatabases,
  parseFolded,
  scopeToForm,
  usageText,
} from "./snippet-dialog";
import type { Snippet } from "./ipc";

const base: Snippet = {
  id: "1",
  abbreviation: "sf",
  body: "SELECT ${*}",
  description: "Consulta base",
  category: "Consultas",
  scope: { profileIds: ["p1"], databases: ["ventas"] },
  shortcut: "Mod-Alt-1",
};

describe("copias", () => {
  it("propone una abreviatura libre", () => {
    expect(freeCopyName("sf", [])).toBe("sf_copia");
    expect(freeCopyName("sf", ["sf_copia"])).toBe("sf_copia2");
    expect(freeCopyName("sf", ["SF_COPIA", "sf_copia2"])).toBe("sf_copia3");
  });

  it("la copia no lleva id ni atajo y no comparte las listas del alcance", () => {
    const copy = duplicateOf(base, ["sf"]);
    expect(copy.id).toBeUndefined();
    expect(copy.shortcut).toBeNull();
    expect(copy.abbreviation).toBe("sf_copia");
    copy.scope.profileIds.push("otro");
    expect(base.scope.profileIds).toEqual(["p1"]);
  });
});

describe("usos", () => {
  const now = 1_000_000;
  it("dice «sin usos» cuando nunca se expandió", () => {
    expect(usageText(undefined, now)).toBe("sin usos");
    expect(usageText({ uses: 0, lastUsedAt: null }, now)).toBe("sin usos");
  });
  it("cuenta y dice cuándo fue", () => {
    expect(usageText({ uses: 142, lastUsedAt: now - 60 }, now)).toBe("142 usos · hoy");
    expect(usageText({ uses: 1, lastUsedAt: now - 90_000 }, now)).toBe("1 uso · ayer");
    expect(usageText({ uses: 3, lastUsedAt: now - 5 * 86_400 }, now)).toBe("3 usos · hace 5 días");
  });
});

describe("alcance del formulario", () => {
  it("«Todos» vacía el alcance aunque haya cosas marcadas", () => {
    expect(formToScope({ only: false, profileIds: ["p1"], databasesText: "ventas" })).toEqual({
      profileIds: [],
      databases: [],
    });
  });
  it("separa las bases y descarta vacías y repetidas", () => {
    expect(parseDatabases(" ventas, ,ventas\nrrhh ")).toEqual(["ventas", "rrhh"]);
  });
  it("ida y vuelta", () => {
    const form = scopeToForm({ profileIds: ["p1"], databases: ["a", "b"] });
    expect(form).toEqual({ only: true, profileIds: ["p1"], databasesText: "a, b" });
    expect(formToScope(form)).toEqual({ profileIds: ["p1"], databases: ["a", "b"] });
    expect(scopeToForm({ profileIds: [], databases: [] }).only).toBe(false);
  });
  it("«solo aquí» toma el servidor y la base de la pestaña, si los hay", () => {
    expect(hereOnly("p1", "ventas")).toEqual({ only: true, profileIds: ["p1"], databasesText: "ventas" });
    expect(hereOnly(null, null)).toEqual({ only: true, profileIds: [], databasesText: "" });
  });
});

describe("importación", () => {
  it("resume lo que pasó", () => {
    expect(importSummary({ added: 3, replaced: 1, skipped: ["a", "b"] })).toBe(
      "3 agregadas, 1 reemplazada, 2 omitidas",
    );
    expect(importSummary({ added: 1, replaced: 0, skipped: [] })).toBe("1 agregada");
  });
});

describe("variables y huecos", () => {
  it("describe lo que preguntará", () => {
    const asked = askedVariables("${e|a|b} ${t@tabla} ${n:5}");
    expect(asked.map((v) => v.kind)).toEqual(["lista", "tabla del catálogo", "texto"]);
    expect(asked[0].detail).toBe("a · b");
    expect(asked[2].detail).toBe("por omisión: 5");
  });
  it("clasifica cada ${…} del cuerpo", () => {
    const kinds = findHoles("${tabla} ${n:5} ${e|a|b} ${f@fecha} ${hoy} ${*}").map((h) => h.kind);
    expect(kinds).toEqual(["hole", "variable", "list", "typed", "auto", "hole"]);
  });
});

describe("carpetas plegadas", () => {
  it("sanea lo guardado", () => {
    expect(parseFolded(null)).toEqual([]);
    expect(parseFolded("no es json")).toEqual([]);
    expect(parseFolded('{"a":1}')).toEqual([]);
    expect(parseFolded('["Consultas", 3, "Datos"]')).toEqual(["Consultas", "Datos"]);
  });
});
