import { describe, expect, it } from "vitest";
import { cursorInfo } from "./cursor-info";

describe("cursorInfo", () => {
  it("empieza en la línea 1, columna 1", () => {
    expect(cursorInfo("SELECT 1", 0, 0)).toEqual({ line: 1, column: 1, selected: 0 });
  });

  it("cuenta las líneas y la columna desde el último salto", () => {
    expect(cursorInfo("a\nbcd\nef", 7, 7)).toEqual({ line: 3, column: 2, selected: 0 });
  });

  it("al inicio de una línea la columna es 1", () => {
    expect(cursorInfo("ab\ncd", 3, 3)).toEqual({ line: 2, column: 1, selected: 0 });
  });

  it("la selección es el valor absoluto, sin importar hacia dónde se arrastró", () => {
    expect(cursorInfo("abcdef", 5, 2).selected).toBe(3);
  });

  it("una posición fuera del texto no rompe", () => {
    expect(cursorInfo("ab", 0, 99)).toEqual({ line: 1, column: 3, selected: 99 });
  });
});
