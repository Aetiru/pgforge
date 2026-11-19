/**
 * Qué decirle al usuario mientras elige la abreviatura o el atajo de una plantilla.
 *
 * Los **errores** son lo que el núcleo rechazaría o lo que no se podría usar nunca; los **avisos**
 * son decisiones que se pueden tomar pero que sorprenden después, y por eso se dicen antes de
 * guardar y no se prohíben. Puro: el diálogo solo pinta el resultado.
 */
import { isReserved } from "./sql-ident";

/** Lo que estas reglas necesitan de una abreviatura; `Snippet` de `ipc/` lo cumple por estructura. */
export interface SnippetLike {
  id?: string;
  abbreviation: string;
  description?: string;
  shortcut?: string | null;
}

export interface Check {
  errors: string[];
  warnings: string[];
}

/**
 * Palabras de SQL que se escriben todo el tiempo aunque PostgreSQL las deje usar como nombre
 * (`update`, `delete`, `insert`… no son reservadas): con una abreviatura así, al apretar Tab para
 * indentar o aceptar la sugerencia se expandiría la plantilla.
 */
const COMMON_KEYWORDS = new Set([
  "select",
  "from",
  "where",
  "join",
  "insert",
  "update",
  "delete",
  "with",
  "order",
  "group",
  "having",
  "limit",
  "offset",
  "create",
  "alter",
  "drop",
  "truncate",
  "begin",
  "commit",
  "rollback",
  "values",
  "set",
  "into",
  "returning",
  "explain",
  "analyze",
  "vacuum",
  "union",
  "case",
  "when",
  "then",
  "end",
  "as",
  "on",
  "in",
  "is",
  "and",
  "or",
  "not",
  "null",
  "true",
  "false",
]);

const WORD = /^[\p{L}\p{N}_]+$/u;

const sameId = (a: string | undefined, b: string | null) => (a ?? null) === b;
const label = (s: SnippetLike) => s.description?.trim() || s.abbreviation;

/** Hasta tres nombres entre comillas, con «y N más» si sobran. */
function listNames(names: string[]): string {
  const shown = names.slice(0, 3).map((n) => `«${n}»`).join(", ");
  return names.length > 3 ? `${shown} y ${names.length - 3} más` : shown;
}

export function checkAbbreviation(
  abbreviation: string,
  existing: SnippetLike[],
  editingId: string | null,
): Check {
  const errors: string[] = [];
  const warnings: string[] = [];
  const value = abbreviation.trim();

  if (value === "") {
    errors.push("La abreviatura no puede estar vacía.");
    return { errors, warnings };
  }
  if (/\s/.test(value)) {
    errors.push("La abreviatura no puede llevar espacios: la expansión mira la palabra pegada al cursor.");
  } else if (!WORD.test(value)) {
    errors.push("Solo letras, números y guion bajo: la expansión mira la palabra pegada al cursor.");
  }
  if (errors.length > 0) return { errors, warnings };

  const others = existing.filter((s) => !sameId(s.id, editingId));
  const lower = value.toLowerCase();

  const clash = others.find((s) => s.abbreviation.trim().toLowerCase() === lower);
  if (clash) {
    errors.push(`Ya existe: ${label(clash)}.`);
    return { errors, warnings };
  }

  const longer = others.filter((s) => s.abbreviation.trim().toLowerCase().startsWith(lower));
  if (longer.length > 0) {
    warnings.push(
      `«${value}» es el comienzo de ${listNames(longer.map((s) => s.abbreviation.trim()))}: al escribirla la lista ofrece varias.`,
    );
  }
  const shorter = others.filter((s) => {
    const other = s.abbreviation.trim().toLowerCase();
    return other !== "" && lower.startsWith(other);
  });
  if (shorter.length > 0) {
    warnings.push(
      `«${value}» empieza como ${listNames(shorter.map((s) => s.abbreviation.trim()))}: al escribirla la lista ofrece las dos.`,
    );
  }

  if (isReserved(lower) || COMMON_KEYWORDS.has(lower)) {
    warnings.push(
      `«${value}» es una palabra de SQL: al apretar Tab se expandiría la plantilla en lugar de lo que querías escribir.`,
    );
  }
  if ([...value].length === 1) {
    warnings.push("Con una sola letra se expande muy fácil sin querer; dos o tres son más seguras.");
  }
  return { errors, warnings };
}

// ---------------------------------------------------------------------------------------------
// Atajos
// ---------------------------------------------------------------------------------------------

const MODIFIERS: Record<string, string> = {
  mod: "mod",
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  meta: "meta",
  cmd: "meta",
  command: "meta",
};

/**
 * La combinación en forma comparable, o `null` si no se lee como una de CodeMirror (`Mod-Alt-1`).
 *
 * Los modificadores se ordenan y se pasan a minúsculas; la tecla se compara sin distinguir
 * mayúsculas. `Mod` y `Ctrl` cuentan como lo mismo: en Windows y Linux —donde `Mod` es `Ctrl`—
 * escribirlas de una u otra forma es la misma pulsación, y en macOS el aviso de más no hace daño.
 */
export function normalizeCombo(combo: string | null | undefined): string | null {
  if (!combo) return null;
  const text = combo.trim();
  if (text === "") return null;

  const mods = new Set<string>();
  let rest = text;
  for (;;) {
    const dash = rest.indexOf("-");
    // El último trozo es la tecla, que puede ser un guion (`Mod--`).
    if (dash <= 0) break;
    const mod = MODIFIERS[rest.slice(0, dash).toLowerCase()];
    if (!mod) break;
    mods.add(mod === "mod" ? "ctrl" : mod);
    rest = rest.slice(dash + 1);
  }
  if (rest === "" || (rest.length > 1 && rest.includes("-")) || MODIFIERS[rest.toLowerCase()]) return null;
  return [...[...mods].sort(), rest.toLowerCase()].join("-");
}

const PRIMARY = new Set(["mod", "ctrl", "alt", "meta"]);

export function checkShortcut(
  combo: string | null,
  snippets: SnippetLike[],
  editingId: string | null,
  builtins: { id: string; label: string; combo: string | null }[],
): Check {
  const errors: string[] = [];
  const warnings: string[] = [];
  if (combo === null || combo.trim() === "") return { errors, warnings };

  const key = normalizeCombo(combo);
  if (key === null) {
    errors.push("No se lee como una combinación: se escribe como Mod-Alt-1 o Ctrl-Shift-k.");
    return { errors, warnings };
  }
  const parts = key.split("-");
  const mods = parts.length > 1 && parts[parts.length - 1] !== "" ? parts.slice(0, -1) : [];
  if (!mods.some((m) => PRIMARY.has(m))) {
    errors.push("Falta Ctrl, Alt o Mod: sin eso se comería el texto que se escribe.");
    return { errors, warnings };
  }

  const clash = snippets.find((s) => !sameId(s.id, editingId) && normalizeCombo(s.shortcut) === key);
  if (clash) {
    errors.push(`Ya lo usa la abreviatura «${clash.abbreviation}».`);
    return { errors, warnings };
  }
  const builtin = builtins.find((b) => normalizeCombo(b.combo) === key);
  if (builtin) warnings.push(`Pisa el atajo de la aplicación «${builtin.label}».`);
  return { errors, warnings };
}
