import { describe, expect, it } from "vitest";

import { columnWidth, gutterWidth } from "./grid-width";

const width = (longest: number, fontSize = 14) =>
  columnWidth({ longest, fontSize, min: 64, max: 340, padding: 20 });

describe("columnWidth", () => {
  it("el encabezado entra entero, que es lo que antes se cortaba", () => {
    // «tipotrabajo» son once caracteres: 11 × 8,4 = 92,4 más el relleno.
    expect(width("tipotrabajo".length)).toBe(113);
  });

  it("una columna angosta no baja del mínimo", () => {
    expect(width(1)).toBe(64);
  });

  it("un texto largo no pasa del máximo", () => {
    expect(width(500)).toBe(340);
  });

  it("con la letra más grande la columna crece, y su techo también", () => {
    // Al agrandar la letra, el mismo texto ocupa más y el máximo se escala con ella: si no, las
    // columnas quedarían del tamaño calculado para la letra chica.
    expect(width(20, 20)).toBeGreaterThan(width(20, 14));
    expect(width(500, 20)).toBe(486);
  });
});

describe("gutterWidth", () => {
  it("crece con la cantidad de dígitos del número de fila", () => {
    expect(gutterWidth(999, 14)).toBeLessThan(gutterWidth(1000, 14));
  });

  it("nunca baja de dos dígitos, para que el «#» del encabezado entre", () => {
    expect(gutterWidth(1, 14)).toBe(gutterWidth(99, 14));
  });
});
