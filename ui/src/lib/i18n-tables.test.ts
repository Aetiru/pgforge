import { describe, expect, it } from "vitest";
import { ACCENTS } from "./accent";
import { RULE_SPECS } from "./alerts";
import { READ_ONLY_LOOK } from "./badges";
import { FILTER_OPS } from "./column-filter";
import { COMPARE_CATEGORIES } from "./compare-kinds";
import { EDITOR_ACTIONS } from "./editor/actions";
import { INDEX_METHODS } from "./index-form";
import { TIMEOUT_OPTIONS } from "./query-timeout";
import { SHORTCUTS } from "./shortcuts";
import { GENERATE_KINDS } from "./sql-generate";
import { PALETTES, SQL_TOKENS } from "./sql-palettes";
import { ORDER_MODES } from "./tree-order";
import { ZONE_MODES } from "./tz";
import { en } from "./locales/en";

/**
 * Las tablas de etiquetas que la interfaz traduce con `t(variable)` (la clave no es un literal, así
 * que `i18n-coverage.test.ts` no las ve). Se recorren los campos de texto visibles y cada uno tiene
 * que estar en el diccionario inglés: si falta, esa pantalla queda en español dentro del inglés.
 */
const VISIBLE = new Set(["label", "hint", "title", "description", "unit", "name", "group"]);

function texts(value: unknown, out: string[] = [], key = ""): string[] {
  if (typeof value === "string") {
    if (VISIBLE.has(key)) out.push(value);
  } else if (Array.isArray(value)) {
    for (const item of value) texts(item, out, key);
  } else if (value && typeof value === "object") {
    for (const name of Object.keys(value)) texts((value as Record<string, unknown>)[name], out, name);
  }
  return out;
}

// Lo que se escribe igual en los dos idiomas (métodos de índice, duraciones, siglas) no necesita
// entrada: `t()` devuelve el mismo texto.
const SAME_IN_BOTH = /^([a-z]+|\d+ (s|min)|UTC)$/;

const TABLES: Record<string, unknown> = {
  ACCENTS,
  RULE_SPECS,
  READ_ONLY_LOOK,
  FILTER_OPS,
  COMPARE_CATEGORIES,
  EDITOR_ACTIONS,
  INDEX_METHODS,
  TIMEOUT_OPTIONS,
  SHORTCUTS,
  GENERATE_KINDS,
  PALETTES,
  SQL_TOKENS,
  ORDER_MODES,
  ZONE_MODES,
};

describe("tablas de etiquetas traducidas con t(variable)", () => {
  for (const [name, table] of Object.entries(TABLES)) {
    it(`${name}: todo texto visible tiene entrada en inglés`, () => {
      const missing = texts(table).filter((text) => !(text in en) && !SAME_IN_BOTH.test(text));
      expect(missing).toEqual([]);
    });
  }
});
