import { describe, expect, it } from "vitest";
import { activeFilters, dayEnd, dayStart, EMPTY_FORM, exportName, toFilter } from "./audit-filter";

describe("audit-filter", () => {
  it("el día 'hasta' incluye el día entero", () => {
    const start = dayStart("2026-03-04")!;
    const end = dayEnd("2026-03-04")!;
    // Un día local mide 24 h salvo que cambie la hora; el rango nunca es menor a 23 h ni cruza al siguiente.
    expect(end - start).toBeGreaterThanOrEqual(23 * 3600 - 1);
    expect(end - start).toBeLessThanOrEqual(25 * 3600 - 1);
    expect(dayStart("2026-03-05")).toBe(end + 1);
  });

  it("una fecha inexistente o mal escrita no filtra", () => {
    expect(dayStart("2026-02-31")).toBeNull();
    expect(dayStart("4/3/2026")).toBeNull();
    expect(dayStart("")).toBeNull();
    expect(dayEnd("nada")).toBeNull();
  });

  it("lo vacío del formulario no filtra", () => {
    const filter = toFilter({ ...EMPTY_FORM, text: "   " });
    expect(filter).toEqual({
      profileId: null,
      database: null,
      from: null,
      to: null,
      source: null,
      environment: null,
      onlyFailed: false,
      text: null,
    });
    expect(activeFilters(EMPTY_FORM)).toBe(0);
  });

  it("cuenta los filtros puestos", () => {
    const form = { ...EMPTY_FORM, profileId: "s1", onlyFailed: true, text: " alter ", from: "2026-01-01" };
    expect(activeFilters(form)).toBe(4);
    expect(toFilter(form).text).toBe("alter");
  });

  it("propone un nombre de archivo con la fecha", () => {
    expect(exportName("csv", new Date(2026, 2, 4))).toBe("auditoria-2026-03-04.csv");
    expect(exportName("json", new Date(2026, 11, 25))).toBe("auditoria-2026-12-25.json");
  });
});
