import { describe, expect, it } from "vitest";
import { parseAccent } from "./accent";

describe("parseAccent", () => {
  it("acepta los acentos conocidos", () => {
    expect(parseAccent("violet")).toBe("violet");
    expect(parseAccent("orange")).toBe("orange");
  });

  it("cualquier otra cosa vuelve al azul de fábrica", () => {
    expect(parseAccent(null)).toBe("blue");
    expect(parseAccent("rojo")).toBe("blue");
  });
});
