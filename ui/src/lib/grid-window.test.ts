import { describe, expect, it } from "vitest";

import { columnRange } from "./grid-window";

/**
 * Una grilla de diez columnas de cien píxeles, con una ventana de trescientos: el caso donde se ve
 * si la cuenta deja un hueco al llegar a un borde.
 */
const widths = Array.from({ length: 10 }, () => 100);
const offsets = widths.map((_, index) => index * 100);

const range = (scrollLeft: number, overscan = 0) =>
  columnRange(offsets, widths, scrollLeft, 300, overscan);

describe("columnRange", () => {
  it("desde el principio dibuja las que entran en la ventana", () => {
    expect(range(0)).toEqual({ first: 0, last: 2 });
  });

  it("una columna cortada al medio cuenta de los dos lados", () => {
    // Con 150 se ve la mitad de la segunda columna y el principio de la quinta.
    expect(range(150)).toEqual({ first: 1, last: 4 });
  });

  it("el borde exacto de una columna no arrastra la anterior", () => {
    expect(range(200)).toEqual({ first: 2, last: 4 });
  });

  it("el margen no se sale de la lista en ninguno de los dos extremos", () => {
    expect(range(0, 2)).toEqual({ first: 0, last: 4 });
    expect(range(700, 2)).toEqual({ first: 5, last: 9 });
  });

  it("con anchos distintos sigue la geometría real y no el índice", () => {
    const mixed = [40, 300, 60, 500];
    const starts = [0, 40, 340, 400];
    expect(columnRange(starts, mixed, 350, 100, 0)).toEqual({ first: 2, last: 3 });
  });

  it("sin columnas no hay nada que dibujar", () => {
    expect(columnRange([], [], 0, 300, 2)).toEqual({ first: 0, last: -1 });
  });
});
