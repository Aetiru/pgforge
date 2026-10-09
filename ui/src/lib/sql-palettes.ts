/**
 * Las paletas del resaltado de SQL y cómo se vuelven CSS.
 *
 * Es puro a propósito: lo que sale de acá es texto que se inyecta en el documento, y un color mal
 * escrito no falla ruidosamente, solo deja una palabra clave sin pintar. Con el generador suelto se
 * prueba que cada paleta trae todos los colores y que lo que viene del almacenamiento (que alguien
 * pudo editar a mano) no llega al CSS sin validar.
 */

export type SqlToken = "keyword" | "string" | "number" | "type" | "function" | "operator" | "comment" | "ident";

export const SQL_TOKENS: { key: SqlToken; label: string }[] = [
  { key: "keyword", label: "Palabras clave" },
  { key: "string", label: "Cadenas" },
  { key: "number", label: "Números, true, false, null" },
  { key: "type", label: "Tipos" },
  { key: "function", label: "Funciones" },
  { key: "operator", label: "Operadores" },
  { key: "comment", label: "Comentarios" },
  { key: "ident", label: "Nombres de tablas y columnas" },
];

export type SqlColors = Record<SqlToken, string>;

export interface SqlPalette {
  id: string;
  name: string;
  description: string;
  light: SqlColors;
  dark: SqlColors;
}

export const PALETTES: SqlPalette[] = [
  {
    id: "actual",
    name: "Actual",
    description: "Azul, verde, violeta, ámbar y celeste: siete tonos distintos.",
    light: { keyword: "#2a5476", string: "#047857", number: "#6d28d9", type: "#b45309", function: "#0369a1", operator: "#626a7a", comment: "#7d8596", ident: "#2c4a66" },
    dark: { keyword: "#6b9cc6", string: "#34d399", number: "#a78bfa", type: "#fbbf24", function: "#38bdf8", operator: "#7d8596", comment: "#626a7a", ident: "#a9c4de" },
  },
  {
    id: "semantica",
    name: "Semántica",
    description: "Un tono por significado, bien separados.",
    light: { keyword: "#1f4e9c", string: "#1a7f37", number: "#b35900", type: "#0b7285", function: "#7a3e9d", operator: "#5c6475", comment: "#8a92a3", ident: "#2a3f5f" },
    dark: { keyword: "#7aa2f7", string: "#7ec787", number: "#f0a35e", type: "#5fc3d3", function: "#c39be0", operator: "#98a0b3", comment: "#6b7385", ident: "#b4c7e8" },
  },
  {
    id: "calma",
    name: "Calma",
    description: "Casi monocromo, con el azul de pgForge en las palabras clave. La más descansada.",
    light: { keyword: "#336791", string: "#5b7f55", number: "#94643a", type: "#66748a", function: "#252a36", operator: "#8890a0", comment: "#9aa1af", ident: "#3c5774" },
    dark: { keyword: "#7fb0d8", string: "#9bbf94", number: "#d0a070", type: "#9aa8bd", function: "#e1e4ea", operator: "#7d8596", comment: "#5f6778", ident: "#a8c3dc" },
  },
  {
    id: "contraste",
    name: "Alto contraste",
    description: "Tonos profundos en claro y brillantes en oscuro.",
    light: { keyword: "#0b2a5b", string: "#0b5c1d", number: "#8a2a00", type: "#5a1a8a", function: "#004a63", operator: "#1d2230", comment: "#4b5262", ident: "#12335c" },
    dark: { keyword: "#9ec1ff", string: "#8ef0a0", number: "#ffb27a", type: "#e0b3ff", function: "#7fdcf5", operator: "#e6e9f0", comment: "#a3abbd", ident: "#c6dcff" },
  },
  {
    id: "calida",
    name: "Cálida",
    description: "Naranja para las palabras clave, al estilo de los IDE de escritorio.",
    light: { keyword: "#a8480f", string: "#3a7a2c", number: "#1a62a0", type: "#7d4a9c", function: "#8c6400", operator: "#6b6254", comment: "#8f8a7f", ident: "#5a4a34" },
    dark: { keyword: "#e8915a", string: "#a9c97a", number: "#6fb3e8", type: "#c79be0", function: "#e5c45f", operator: "#a39d8f", comment: "#7a766b", ident: "#e2cdb0" },
  },
];

export const CUSTOM_ID = "custom";

export type KeywordWeight = 400 | 500 | 700;

export interface SqlThemePrefs {
  /** Un `id` de `PALETTES` o `custom`. */
  palette: string;
  /** Los colores de «Personalizada»; arranca como una copia de «Actual». */
  custom: { light: SqlColors; dark: SqlColors };
  keywordWeight: KeywordWeight;
  commentItalic: boolean;
  /** Los nombres de tablas y columnas con color propio, y no el del texto. */
  identTint: boolean;
}

const copy = (colors: SqlColors): SqlColors => ({ ...colors });

export function defaultPrefs(): SqlThemePrefs {
  return {
    palette: "actual",
    custom: { light: copy(PALETTES[0].light), dark: copy(PALETTES[0].dark) },
    keywordWeight: 700,
    commentItalic: true,
    identTint: true,
  };
}

const HEX = /^#[0-9a-f]{6}$/i;

function validColors(value: unknown, fallback: SqlColors): SqlColors {
  const out = copy(fallback);
  if (typeof value !== "object" || value === null) return out;
  for (const { key } of SQL_TOKENS) {
    const candidate = (value as Record<string, unknown>)[key];
    if (typeof candidate === "string" && HEX.test(candidate)) out[key] = candidate;
  }
  return out;
}

/**
 * Lo que se leyó del almacenamiento, saneado: una preferencia que alguien editó a mano (o de una
 * versión vieja) no puede dejar el editor sin colores ni meter texto raro en el CSS inyectado.
 */
export function parsePrefs(raw: unknown): SqlThemePrefs {
  const base = defaultPrefs();
  if (typeof raw !== "object" || raw === null) return base;
  const value = raw as Record<string, unknown>;
  const known = value.palette === CUSTOM_ID || PALETTES.some((p) => p.id === value.palette);
  const custom = (value.custom ?? {}) as Record<string, unknown>;
  return {
    palette: known ? (value.palette as string) : base.palette,
    custom: {
      light: validColors(custom.light, base.custom.light),
      dark: validColors(custom.dark, base.custom.dark),
    },
    keywordWeight: value.keywordWeight === 400 || value.keywordWeight === 500 ? value.keywordWeight : 700,
    commentItalic: value.commentItalic !== false,
    identTint: value.identTint !== false,
  };
}

export function colorsOf(prefs: SqlThemePrefs, theme: "light" | "dark"): SqlColors {
  if (prefs.palette === CUSTOM_ID) return prefs.custom[theme];
  return (PALETTES.find((p) => p.id === prefs.palette) ?? PALETTES[0])[theme];
}

function block(prefs: SqlThemePrefs, theme: "light" | "dark"): string {
  const c = colorsOf(prefs, theme);
  return [
    `--cm-keyword: ${c.keyword}`,
    `--cm-string: ${c.string}`,
    `--cm-number: ${c.number}`,
    `--cm-type: ${c.type}`,
    `--cm-function: ${c.function}`,
    `--cm-operator: ${c.operator}`,
    `--cm-comment: ${c.comment}`,
    // Sin tinte, el nombre toma el color del texto: es lo que pasaba antes de tener esta opción.
    `--cm-ident: ${prefs.identTint ? c.ident : "var(--cm-text)"}`,
  ].join("; ");
}

/**
 * El CSS que pisa los `--cm-*` de `app.css`. Lleva `html` por delante para ganar en especificidad
 * sin depender de qué hoja se leyó última.
 */
export function paletteCss(prefs: SqlThemePrefs): string {
  const shared = `--cm-keyword-weight: ${prefs.keywordWeight}; --cm-comment-style: ${prefs.commentItalic ? "italic" : "normal"}`;
  return `html:root { ${block(prefs, "light")}; ${shared} }\nhtml[data-theme="dark"] { ${block(prefs, "dark")} }\n`;
}
