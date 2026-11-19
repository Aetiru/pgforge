import { describe, expect, it } from "vitest";
import { checkAbbreviation, checkShortcut, normalizeCombo, type SnippetLike } from "./snippet-check";

const existing: SnippetLike[] = [
  { id: "1", abbreviation: "sf", description: "Consulta base" },
  { id: "2", abbreviation: "sc", description: "Contar filas" },
  { id: "3", abbreviation: "ij", description: "" },
];

describe("checkAbbreviation", () => {
  it("una abreviatura libre no dice nada", () => {
    expect(checkAbbreviation("top", existing, null)).toEqual({ errors: [], warnings: [] });
  });

  it("rechaza la vacía, con espacios y con símbolos", () => {
    expect(checkAbbreviation("  ", existing, null).errors).toHaveLength(1);
    expect(checkAbbreviation("a b", existing, null).errors[0]).toContain("espacios");
    expect(checkAbbreviation("a-b", existing, null).errors[0]).toContain("Solo letras");
  });

  it("acepta letras con acento, números y guion bajo", () => {
    expect(checkAbbreviation("año_2", existing, null).errors).toEqual([]);
  });

  it("rechaza la repetida sin distinguir mayúsculas y dice cuál es", () => {
    expect(checkAbbreviation("SF", existing, null).errors).toEqual(["Ya existe: Consulta base."]);
    // Sin descripción, dice la abreviatura.
    expect(checkAbbreviation("ij", existing, null).errors).toEqual(["Ya existe: ij."]);
  });

  it("editar la propia no cuenta como repetida", () => {
    expect(checkAbbreviation("sf", existing, "1").errors).toEqual([]);
  });

  it("avisa cuando es el comienzo de otra y cuando otra es el comienzo suyo", () => {
    expect(checkAbbreviation("s", existing, null).warnings.join(" ")).toContain("«sf»");
    expect(checkAbbreviation("sfx", existing, null).warnings.join(" ")).toContain("empieza como «sf»");
  });

  it("avisa con las palabras de SQL, reservadas o no", () => {
    expect(checkAbbreviation("select", existing, null).warnings[0]).toContain("palabra de SQL");
    expect(checkAbbreviation("update", existing, null).warnings[0]).toContain("palabra de SQL");
    expect(checkAbbreviation("sel", existing, null).warnings).toEqual([]);
  });

  it("avisa con una sola letra", () => {
    expect(checkAbbreviation("z", existing, null).warnings[0]).toContain("una sola letra");
  });

  it("nombra a lo sumo tres y cuenta el resto", () => {
    const many = ["ab1", "ab2", "ab3", "ab4", "ab5"].map((abbreviation, i) => ({ id: String(i), abbreviation }));
    expect(checkAbbreviation("ab", many, null).warnings[0]).toContain("y 2 más");
  });
});

describe("normalizeCombo", () => {
  it("ordena los modificadores y no distingue mayúsculas", () => {
    expect(normalizeCombo("Alt-Mod-1")).toBe(normalizeCombo("mod-alt-1"));
    expect(normalizeCombo("Mod-Shift-K")).toBe("ctrl-shift-k");
  });

  it("toma Mod y Ctrl por lo mismo, y Cmd por Meta", () => {
    expect(normalizeCombo("Mod-j")).toBe(normalizeCombo("Ctrl-j"));
    expect(normalizeCombo("Cmd-j")).toBe("meta-j");
  });

  it("lee la tecla guion", () => {
    expect(normalizeCombo("Mod--")).toBe("ctrl--");
  });

  it("rechaza lo que no es una combinación", () => {
    expect(normalizeCombo("")).toBeNull();
    expect(normalizeCombo("Mod-")).toBeNull();
    expect(normalizeCombo("Mod-Alt")).toBeNull();
    expect(normalizeCombo("Mod-a-b")).toBeNull();
  });
});

describe("checkShortcut", () => {
  const builtins = [{ id: "app.palette", label: "Paleta de comandos", combo: "Mod-k" }];
  const withShortcut: SnippetLike[] = [{ id: "1", abbreviation: "sf", shortcut: "Mod-Alt-1" }];

  it("sin atajo no hay nada que decir", () => {
    expect(checkShortcut(null, [], null, builtins)).toEqual({ errors: [], warnings: [] });
  });

  it("exige Ctrl, Alt o Mod", () => {
    expect(checkShortcut("Shift-a", [], null, builtins).errors[0]).toContain("Falta Ctrl");
    expect(checkShortcut("a", [], null, builtins).errors[0]).toContain("Falta Ctrl");
  });

  it("rechaza lo que no se lee", () => {
    expect(checkShortcut("Mod-", [], null, builtins).errors[0]).toContain("No se lee");
  });

  it("choca con otra abreviatura como error, escrito de otra forma también", () => {
    expect(checkShortcut("Alt-Ctrl-1", withShortcut, null, builtins).errors[0]).toContain("«sf»");
    expect(checkShortcut("Mod-Alt-1", withShortcut, "1", builtins).errors).toEqual([]);
  });

  it("choca con un atajo de la aplicación como aviso, con su nombre", () => {
    const result = checkShortcut("Ctrl-k", [], null, builtins);
    expect(result.errors).toEqual([]);
    expect(result.warnings[0]).toContain("Paleta de comandos");
  });
});
