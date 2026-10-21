import { describe, expect, it } from "vitest";
import {
  SHORTCUTS,
  comboFromEvent,
  effective,
  findConflicts,
  formatCombo,
  matchesEvent,
  normalizeCombo,
  parseOverrides,
  validateCombo,
} from "./shortcuts";

const ev = (init: Partial<Parameters<typeof comboFromEvent>[0]> & { key: string }) => ({
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...init,
});

describe("normalizeCombo", () => {
  it("deja un orden canónico y la tecla en minúscula", () => {
    expect(normalizeCombo("Shift-Mod-F")).toBe("Mod-Shift-f");
    expect(normalizeCombo("Mod-Shift-F")).toBe("Mod-Shift-f");
    expect(normalizeCombo("mod-alt-shift-x")).toBe("Mod-Alt-Shift-x");
  });

  it("entiende la tecla guion y las de función", () => {
    expect(normalizeCombo("Mod--")).toBe("Mod--");
    expect(normalizeCombo("F1")).toBe("F1");
    expect(normalizeCombo("Mod-Enter")).toBe("Mod-Enter");
  });

  it("rechaza lo que no es una tecla", () => {
    expect(normalizeCombo("Mod-Nada")).toBeNull();
    expect(normalizeCombo("")).toBeNull();
  });
});

describe("comboFromEvent / matchesEvent", () => {
  it("Ctrl y Meta son lo mismo (Mod)", () => {
    expect(comboFromEvent(ev({ key: "k", ctrlKey: true }))).toBe("Mod-k");
    expect(comboFromEvent(ev({ key: "K", metaKey: true }))).toBe("Mod-k");
  });

  it("Ctrl+Mayús+F llega como «F» y se normaliza", () => {
    expect(matchesEvent(ev({ key: "F", ctrlKey: true, shiftKey: true }), "Mod-Shift-f")).toBe(true);
  });

  it("un modificador solo no es una combinación", () => {
    expect(comboFromEvent(ev({ key: "Control", ctrlKey: true }))).toBeNull();
  });

  it("con Alt usa la posición física, no el carácter compuesto", () => {
    expect(comboFromEvent(ev({ key: "∫", code: "KeyB", altKey: true }))).toBe("Alt-b");
  });

  it("la barra espaciadora se llama Space", () => {
    expect(comboFromEvent(ev({ key: " " }))).toBe("Space");
  });

  it("sin asignar nunca coincide", () => {
    expect(matchesEvent(ev({ key: "k", ctrlKey: true }), null)).toBe(false);
  });
});

describe("validateCombo", () => {
  it("exige Ctrl o Alt salvo en teclas de función", () => {
    expect(validateCombo("Mod-k")).toBeNull();
    expect(validateCombo("Alt-x")).toBeNull();
    expect(validateCombo("F5")).toBeNull();
    expect(validateCombo("k")).not.toBeNull();
    expect(validateCombo("Shift-k")).not.toBeNull();
  });

  it("rechaza símbolos con Mayús", () => {
    expect(validateCombo("Mod-Shift-/")).not.toBeNull();
  });
});

describe("formatCombo", () => {
  it("lo escribe en español", () => {
    expect(formatCombo("Mod-Shift-f")).toBe("Ctrl+Mayús+F");
    expect(formatCombo("Alt-ArrowLeft")).toBe("Alt+←");
    expect(formatCombo(null)).toBe("Sin asignar");
  });
});

describe("tabla de atajos", () => {
  it("no tiene ids repetidos ni combinaciones de fábrica inválidas", () => {
    const ids = SHORTCUTS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const def of SHORTCUTS) {
      if (def.default !== null) expect(normalizeCombo(def.default), def.id).toBe(def.default);
    }
  });

  it("de fábrica no se pisa nada", () => {
    expect([...findConflicts(effective({}))]).toEqual([]);
  });
});

describe("effective / findConflicts", () => {
  it("lo reasignado gana y lo fijo no se puede reasignar", () => {
    const map = effective({ "app.palette": "Mod-j", "grid.copy": "Mod-x", "app.inspector": null });
    expect(map.get("app.palette")).toBe("Mod-j");
    expect(map.get("grid.copy")).toBe("Mod-c");
    expect(map.get("app.inspector")).toBeNull();
  });

  it("detecta dos atajos globales con la misma tecla", () => {
    const conflicts = findConflicts(effective({ "app.palette": "Mod-q" }));
    expect(conflicts.get("app.palette")).toEqual(["app.newQuery"]);
    expect(conflicts.get("app.newQuery")).toEqual(["app.palette"]);
  });

  it("un atajo global choca con uno fijo de la grilla", () => {
    expect(findConflicts(effective({ "app.open": "Mod-c" })).get("app.open")).toEqual(["grid.copy"]);
  });

  it("el mismo atajo en contextos que no se solapan no es conflicto", () => {
    const conflicts = findConflicts(effective({ "editor.format": "Mod-Shift-Enter", "editor.runScript": "Mod-j" }));
    expect(conflicts.size).toBe(0);
  });

  it("dos sin asignar no chocan", () => {
    expect(findConflicts(effective({ "app.palette": null, "app.search": null })).size).toBe(0);
  });
});

describe("parseOverrides", () => {
  it("sanea lo guardado", () => {
    const raw = JSON.stringify({
      "app.palette": "mod-j",
      "app.search": null,
      "grid.copy": "Mod-x",
      "app.open": "k",
      "nope": "Mod-z",
    });
    expect(parseOverrides(raw)).toEqual({ "app.palette": "Mod-j", "app.search": null });
  });

  it("tolera basura", () => {
    expect(parseOverrides("{roto")).toEqual({});
    expect(parseOverrides("[]")).toEqual({});
    expect(parseOverrides(null)).toEqual({});
  });
});
