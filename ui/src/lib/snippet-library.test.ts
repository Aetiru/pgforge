import { describe, expect, it } from "vitest";
import { checkAbbreviation } from "./snippet-check";
import { LIBRARY, libraryAvailable, libraryCategories } from "./snippet-library";
import { assessRisks } from "./statement-risk";
import { autoVariablesOf, previewBody, variablesOf } from "./snippet-vars";

/** Las de fábrica de `sql::snippet::defaults`: la biblioteca no las pisa. */
const FACTORY = ["sf", "sc", "ij", "cte"];

describe("LIBRARY", () => {
  it("tiene una treintena de plantillas en las cinco categorías", () => {
    expect(LIBRARY.length).toBeGreaterThanOrEqual(28);
    expect(libraryCategories()).toEqual([
      "Consultas",
      "Datos",
      "Diagnóstico",
      "Mantenimiento",
      "Estructura",
    ]);
  });

  it("las abreviaturas son válidas, únicas y no se avisan entre sí ni pisan las de fábrica", () => {
    const ids = new Set<string>();
    for (const entry of LIBRARY) {
      expect(ids.has(entry.id), `id repetido ${entry.id}`).toBe(false);
      ids.add(entry.id);
      expect(FACTORY).not.toContain(entry.abbreviation);
      const others = LIBRARY.filter((other) => other.id !== entry.id);
      const result = checkAbbreviation(entry.abbreviation, others, null);
      expect(result, entry.abbreviation).toEqual({ errors: [], warnings: [] });
    }
  });

  it("todas tienen descripción y cuerpo", () => {
    for (const entry of LIBRARY) {
      expect(entry.description.trim(), entry.abbreviation).not.toBe("");
      expect(entry.body.trim(), entry.abbreviation).not.toBe("");
    }
  });

  it("las variables se leen y la vista previa no deja huecos sin resolver", () => {
    for (const entry of LIBRARY) {
      expect(() => variablesOf(entry.body)).not.toThrow();
      const preview = previewBody(entry.body);
      expect(preview, entry.abbreviation).not.toContain("${");
      expect(preview.trim(), entry.abbreviation).not.toBe("");
      // Ninguna usa automáticos: son para que cada uno los escriba en las suyas.
      expect(autoVariablesOf(entry.body)).toEqual([]);
    }
  });

  it("ninguna destruye sin WHERE y las que escriben o borran avisan", () => {
    for (const entry of LIBRARY) {
      const risks = assessRisks(previewBody(entry.body));
      expect(risks, entry.abbreviation).toEqual([]);
      if (/\b(DELETE|UPDATE|ON CONFLICT|pg_terminate_backend)\b/i.test(entry.body)) {
        expect(entry.body, entry.abbreviation).toContain("-- cuidado:");
      }
    }
  });

  it("las que no corren en una transacción lo dicen", () => {
    for (const entry of LIBRARY.filter((e) => /CONCURRENTLY/.test(e.body))) {
      expect(entry.body, entry.abbreviation).toContain("-- no puede correr dentro de una transacción");
    }
  });

  it("no usa nada que solo exista en las versiones nuevas", () => {
    const joined = LIBRARY.map((e) => e.body).join("\n");
    // MERGE, JSON_TABLE, pg_stat_io y pg_stat_checkpointer son de PG 15 a 17.
    expect(joined).not.toMatch(/\bMERGE\b|JSON_TABLE|pg_stat_io|pg_stat_checkpointer/i);
  });
});

describe("libraryAvailable", () => {
  it("deja afuera las ya instaladas, sin distinguir mayúsculas", () => {
    const available = libraryAvailable(LIBRARY, [{ abbreviation: "ULT" }, { abbreviation: "pag" }]);
    expect(available.length).toBe(LIBRARY.length - 2);
    expect(available.some((e) => e.abbreviation === "ult")).toBe(false);
  });

  it("con nada instalado ofrece todas", () => {
    expect(libraryAvailable(LIBRARY, [])).toHaveLength(LIBRARY.length);
  });
});
