import { describe, expect, it } from "vitest";
import { hiddenKey, parseHidden, visibleSchemas } from "./hidden-schemas";

describe("hidden-schemas", () => {
  it("separa servidor y base", () => {
    expect(hiddenKey("a", "x")).not.toBe(hiddenKey("a", "y"));
    expect(hiddenKey("a", "x")).not.toBe(hiddenKey("b", "x"));
  });

  it("un esquema nuevo aparece solo: se oculta lo listado, no se muestra lo listado", () => {
    const nodes = [{ label: "public" }, { label: "audit" }, { label: "nuevo" }];
    const { shown, hiddenCount } = visibleSchemas(nodes, ["audit"]);
    expect(shown.map((n) => n.label)).toEqual(["public", "nuevo"]);
    expect(hiddenCount).toBe(1);
  });

  it("sin nada oculto devuelve todo", () => {
    expect(visibleSchemas([{ label: "a" }], []).hiddenCount).toBe(0);
  });

  it("sanea lo guardado", () => {
    expect(parseHidden(JSON.stringify({ k: ["a", 3, "a"], malo: "x", vacio: [] }))).toEqual({ k: ["a"] });
    expect(parseHidden("???")).toEqual({});
    expect(parseHidden(null)).toEqual({});
  });
});
