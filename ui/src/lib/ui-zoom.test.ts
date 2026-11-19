import { describe, expect, it } from "vitest";
import { clampZoom, storedZoom } from "./ui-zoom.svelte";

describe("ui-zoom", () => {
  it("acota entre 80 % y 150 % y redondea a décimas", () => {
    expect(clampZoom(0.3)).toBe(0.8);
    expect(clampZoom(9)).toBe(1.5);
    expect(clampZoom(1.2000000000000002)).toBe(1.2);
  });

  it("un valor guardado roto vuelve a 100 %", () => {
    expect(storedZoom(null)).toBe(1);
    expect(storedZoom("abc")).toBe(1);
    expect(storedZoom("1.3")).toBe(1.3);
  });
});
