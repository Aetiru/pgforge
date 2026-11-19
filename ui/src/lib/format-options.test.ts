import { describe, expect, it } from "vitest";
import { DEFAULT_FORMAT_OPTIONS } from "./ipc";
import { isDefaultFormat, parseStoredFormatOptions, sanitizeFormatOptions } from "./format-options";

describe("opciones del formateador guardadas", () => {
  it("sin nada guardado o con JSON roto usa los valores de fábrica", () => {
    expect(parseStoredFormatOptions(null)).toEqual(DEFAULT_FORMAT_OPTIONS);
    expect(parseStoredFormatOptions("{no es json")).toEqual(DEFAULT_FORMAT_OPTIONS);
    expect(parseStoredFormatOptions("42")).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  it("conserva lo válido y reemplaza solo lo inválido", () => {
    const options = sanitizeFormatOptions({
      keywordCase: "lower",
      indent: "7",
      linesBetweenQueries: 2,
      commaFirst: "si",
    });
    expect(options).toEqual({
      keywordCase: "lower",
      indent: DEFAULT_FORMAT_OPTIONS.indent,
      linesBetweenQueries: 2,
      commaFirst: DEFAULT_FORMAT_OPTIONS.commaFirst,
    });
  });

  it("rechaza líneas fuera de rango o no enteras", () => {
    for (const bad of [-1, 4, 1.5, "x", null]) {
      expect(sanitizeFormatOptions({ linesBetweenQueries: bad }).linesBetweenQueries).toBe(
        DEFAULT_FORMAT_OPTIONS.linesBetweenQueries,
      );
    }
    expect(sanitizeFormatOptions({ linesBetweenQueries: 0 }).linesBetweenQueries).toBe(0);
  });

  it("no comparte el objeto de fábrica con quien lo modifique", () => {
    const options = parseStoredFormatOptions(null);
    options.keywordCase = "lower";
    expect(DEFAULT_FORMAT_OPTIONS.keywordCase).toBe("upper");
  });

  it("reconoce los valores de fábrica", () => {
    expect(isDefaultFormat({ ...DEFAULT_FORMAT_OPTIONS })).toBe(true);
    expect(isDefaultFormat({ ...DEFAULT_FORMAT_OPTIONS, commaFirst: true })).toBe(false);
  });
});
