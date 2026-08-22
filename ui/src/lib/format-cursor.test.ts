import { describe, expect, it } from "vitest";

import { offsetOfStatement } from "./format-cursor";

describe("offsetOfStatement", () => {
  it("la primera sentencia empieza en cero", () => {
    expect(offsetOfStatement("SELECT 1;\n\nSELECT 2", 0)).toBe(0);
  });

  it("salta el separador de cada sentencia anterior", () => {
    const formatted = "SELECT 1;\n\nSELECT 2;\n\nSELECT 3";
    expect(offsetOfStatement(formatted, 1)).toBe("SELECT 1;\n\n".length);
    expect(offsetOfStatement(formatted, 2)).toBe("SELECT 1;\n\nSELECT 2;\n\n".length);
  });

  it("un índice fuera de rango cae en la última sentencia", () => {
    const formatted = "SELECT 1;\n\nSELECT 2";
    expect(offsetOfStatement(formatted, 5)).toBe("SELECT 1;\n\n".length);
  });

  it("un índice negativo cae en la primera", () => {
    expect(offsetOfStatement("SELECT 1;\n\nSELECT 2", -1)).toBe(0);
  });

  it("un solo bloque no tiene dónde saltar", () => {
    expect(offsetOfStatement("SELECT 1", 3)).toBe(0);
  });
});
