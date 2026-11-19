import { describe, expect, it } from "vitest";
import {
  filterSnippets,
  groupByCategory,
  scopeLabel,
  scopeMatches,
  sortSnippets,
  unusedIds,
  visibleFor,
  type SnippetItem,
} from "./snippet-list";

const item = (abbreviation: string, extra: Partial<SnippetItem> = {}): SnippetItem => ({
  id: abbreviation,
  abbreviation,
  body: `SELECT '${abbreviation}'`,
  description: "",
  ...extra,
});

const ITEMS = [
  item("sf", { description: "Consulta base", category: "Consultas" }),
  item("cte", { description: "Expresión de tabla común", category: "Consultas" }),
  item("vac", { description: "Mantenimiento", category: "Mantenimiento", body: "VACUUM ANALYZE" }),
  item("zeta"),
];

describe("filterSnippets", () => {
  it("sin consulta devuelve todo", () => {
    expect(filterSnippets(ITEMS, "  ")).toHaveLength(4);
  });

  it("busca sin acentos ni mayúsculas, en descripción, categoría y cuerpo", () => {
    expect(filterSnippets(ITEMS, "EXPRESION").map((i) => i.abbreviation)).toEqual(["cte"]);
    expect(filterSnippets(ITEMS, "mantenimiento").map((i) => i.abbreviation)).toEqual(["vac"]);
    expect(filterSnippets(ITEMS, "vacuum").map((i) => i.abbreviation)).toEqual(["vac"]);
  });

  it("todas las palabras tienen que estar", () => {
    expect(filterSnippets(ITEMS, "consulta base").map((i) => i.abbreviation)).toEqual(["sf"]);
    expect(filterSnippets(ITEMS, "consulta vacuum")).toEqual([]);
  });
});

describe("sortSnippets", () => {
  const usage = {
    sf: { uses: 5, lastUsedAt: 100 },
    cte: { uses: 9, lastUsedAt: 50 },
    vac: { uses: 5, lastUsedAt: 200 },
  };

  it("por nombre, sin tocar el original", () => {
    const sorted = sortSnippets(ITEMS, "name");
    expect(sorted.map((i) => i.abbreviation)).toEqual(["cte", "sf", "vac", "zeta"]);
    expect(ITEMS[0].abbreviation).toBe("sf");
  });

  it("por usos, con el nombre de desempate y las no usadas al final", () => {
    expect(sortSnippets(ITEMS, "uses", usage).map((i) => i.abbreviation)).toEqual(["cte", "sf", "vac", "zeta"]);
  });

  it("por recientes, con las que nunca se usaron al final", () => {
    expect(sortSnippets(ITEMS, "recent", usage).map((i) => i.abbreviation)).toEqual(["vac", "sf", "cte", "zeta"]);
  });
});

describe("groupByCategory", () => {
  it("ordena las categorías y deja «Sin categoría» al final", () => {
    const groups = groupByCategory(ITEMS);
    expect(groups.map((g) => g.category)).toEqual(["Consultas", "Mantenimiento", "Sin categoría"]);
    expect(groups[0].items.map((i) => i.abbreviation)).toEqual(["sf", "cte"]);
  });

  it("una categoría en blanco cuenta como sin categoría", () => {
    expect(groupByCategory([item("a", { category: "  " })])[0].category).toBe("Sin categoría");
  });
});

describe("alcance", () => {
  const everywhere = item("a");
  const prod = item("b", { scope: { profileIds: ["p1"], databases: [] } });
  const ventas = item("c", { scope: { profileIds: [], databases: ["ventas"] } });
  const both = item("d", { scope: { profileIds: ["p1"], databases: ["ventas"] } });

  it("un alcance vacío vale en todos lados, incluso sin saber dónde se está", () => {
    expect(scopeMatches(everywhere, null, null)).toBe(true);
    expect(scopeMatches(item("x", { scope: { profileIds: [], databases: [] } }), "p9", "x")).toBe(true);
  });

  it("con perfiles, el actual tiene que estar", () => {
    expect(scopeMatches(prod, "p1", "otra")).toBe(true);
    expect(scopeMatches(prod, "p2", "ventas")).toBe(false);
    expect(scopeMatches(prod, null, null)).toBe(false);
  });

  it("con bases, la actual tiene que estar", () => {
    expect(scopeMatches(ventas, "p2", "ventas")).toBe(true);
    expect(scopeMatches(ventas, "p2", "otra")).toBe(false);
  });

  it("con las dos, valen las dos", () => {
    expect(scopeMatches(both, "p1", "ventas")).toBe(true);
    expect(scopeMatches(both, "p1", "otra")).toBe(false);
    expect(scopeMatches(both, "p2", "ventas")).toBe(false);
  });

  it("visibleFor filtra la lista", () => {
    expect(visibleFor([everywhere, prod, ventas, both], "p1", "ventas")).toHaveLength(4);
    expect(visibleFor([everywhere, prod, ventas, both], "p2", "otra").map((i) => i.abbreviation)).toEqual(["a"]);
  });

  it("scopeLabel dice dónde vale", () => {
    const name = (id: string) => (id === "p1" ? "Producción ventas" : null);
    expect(scopeLabel(everywhere, name)).toBe("Todos los servidores");
    expect(scopeLabel(prod, name)).toBe("Producción ventas");
    expect(scopeLabel(ventas, name)).toBe("base ventas");
    expect(scopeLabel(both, name)).toBe("Producción ventas · base ventas");
    expect(scopeLabel(item("z", { scope: { profileIds: ["p7"], databases: ["a", "b"] } }), name)).toBe(
      "servidor desconocido · bases a, b",
    );
  });
});

describe("unusedIds", () => {
  const NOW = 10_000_000;
  const DAY = 86_400;
  const usage = {
    sf: { uses: 4, lastUsedAt: NOW - 5 * DAY },
    cte: { uses: 2, lastUsedAt: NOW - 90 * DAY },
    vac: { uses: 0, lastUsedAt: null },
  };

  it("propone las nunca usadas y las que hace más de N días que no se usan", () => {
    expect(unusedIds(ITEMS, usage, 60, NOW).sort()).toEqual(["cte", "vac", "zeta"]);
  });

  it("con otro plazo cambia el corte", () => {
    expect(unusedIds(ITEMS, usage, 120, NOW).sort()).toEqual(["vac", "zeta"]);
  });

  it("ignora las que todavía no tienen id", () => {
    expect(unusedIds([{ abbreviation: "n", body: "x", description: "" }], {}, 60, NOW)).toEqual([]);
  });
});
