/**
 * Los atajos de teclado de la aplicación: **una sola tabla** que consultan los componentes.
 *
 * Antes cada atajo vivía donde se atendía —el `onKeydown` de `App.svelte`, el keymap del editor,
 * el `title` de un botón, la `hint` de la paleta—, y decir cuáles existen exigía leer todo eso.
 * Acá está la lista, con su combinación de fábrica, qué hace y en qué contexto vale; lo
 * reasignable se guarda como preferencia (`shortcuts.svelte.ts`) y el resto aparece en la chuleta
 * como informativo.
 *
 * Una combinación se escribe como la entiende CodeMirror (`Mod-Shift-f`): `Mod` es `Ctrl` (o `⌘`
 * en macOS), después `Alt`, `Shift` y la tecla, siempre en ese orden. Así el mismo texto sirve
 * para el keymap del editor —que sigue en `Prec.highest` y devolviendo `false` cuando no le
 * toca— y para el `keydown` de la ventana, sin dos notaciones que mantener sincronizadas.
 *
 * Todo acá es puro: lo que depende de `localStorage` o del DOM está en `shortcuts.svelte.ts`.
 */


import { t } from "./i18n.svelte";
/** Dónde vale el atajo. `global` lo atiende la ventana, así que se solapa con todo lo demás. */
export type Scope = "global" | "editor" | "results" | "grid" | "tree";

export interface ShortcutDef {
  id: string;
  label: string;
  group: string;
  scope: Scope;
  /** Combinación de fábrica; `null` = sin asignar. */
  default: string | null;
  /** No se puede reasignar: lo atiende un componente por su cuenta (se muestra, no se edita). */
  fixed?: boolean;
}

export const SHORTCUTS: ShortcutDef[] = [
  { id: "app.palette", get label() { return t("Paleta de comandos"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-k" },
  { id: "app.search", get label() { return t("Buscar objetos en el servidor"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-Shift-p" },
  { id: "app.newQuery", get label() { return t("Nueva consulta"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-q" },
  { id: "app.open", get label() { return t("Abrir un archivo SQL"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-o" },
  { id: "app.save", get label() { return t("Guardar la consulta"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-s" },
  { id: "app.saveAs", get label() { return t("Guardar como…"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-Shift-s" },
  { id: "app.sidebar", get label() { return t("Mostrar u ocultar el panel lateral"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-b" },
  { id: "app.inspector", get label() { return t("Mostrar u ocultar el inspector"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-i" },
  { id: "app.tabs", get label() { return t("Buscar entre las pestañas abiertas"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-p" },
  { id: "app.lastTab", get label() { return t("Saltar a la pestaña anterior"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-Tab" },
  { id: "app.back", get label() { return t("Volver a la pestaña que se miraba antes"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Alt-Shift-ArrowLeft" },
  { id: "app.forward", get label() { return t("Avanzar a la pestaña siguiente del recorrido"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Alt-Shift-ArrowRight" },
  { id: "app.zoomIn", get label() { return t("Agrandar toda la interfaz"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-Alt-=" },
  { id: "app.zoomOut", get label() { return t("Achicar toda la interfaz"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-Alt--" },
  { id: "app.zoomReset", get label() { return t("Interfaz a su tamaño (100 %)"); }, get group() { return t("Aplicación"); }, scope: "global", default: "Mod-Alt-0" },
  { id: "app.shortcuts", get label() { return t("Esta chuleta de atajos"); }, get group() { return t("Aplicación"); }, scope: "global", default: "F1" },

  { id: "editor.run", get label() { return t("Ejecutar la sentencia o la selección"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Enter" },
  { id: "editor.runScript", get label() { return t("Ejecutar el script entero"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Shift-Enter" },
  { id: "editor.commit", get label() { return t("Confirmar la transacción (Commit)"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Alt-Enter" },
  { id: "editor.rollback", get label() { return t("Revertir la transacción (Rollback)"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Alt-Backspace" },
  { id: "editor.format", get label() { return t("Formatear el SQL"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Shift-f" },
  { id: "editor.runToCursor", get label() { return t("Ejecutar hasta el cursor (inicio del script hasta el fin de la sentencia actual)"); }, get group() { return t("Editor"); }, scope: "editor", default: "Alt-Shift-Enter" },
  { id: "editor.runFromCursor", get label() { return t("Ejecutar desde el cursor (desde el inicio de la sentencia actual hasta el final)"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Alt-Shift-Enter" },
  { id: "editor.expandStar", get label() { return t("Expandir el * a la lista de columnas"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Shift-e" },
  { id: "editor.rename", get label() { return t("Renombrar el alias bajo el cursor"); }, get group() { return t("Editor"); }, scope: "editor", default: "F2" },
  { id: "editor.outline", get label() { return t("Mostrar u ocultar el esquema del script"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Shift-l" },
  { id: "editor.insertObject", get label() { return t("Insertar un objeto buscándolo"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Shift-o" },
  { id: "editor.pasteList", get label() { return t("Pegar el portapapeles como lista IN"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Shift-v", fixed: true },
  { id: "editor.versions", get label() { return t("Versiones anteriores de este texto"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Alt-h" },
  { id: "editor.saveSnippet", get label() { return t("Guardar la selección como abreviatura"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Shift-a" },
  { id: "editor.snippetPicker", get label() { return t("Elegir una abreviatura (con vista previa)"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-Alt-j" },
  { id: "editor.expand", get label() { return t("Expandir una abreviatura / saltar al hueco siguiente"); }, get group() { return t("Editor"); }, scope: "editor", default: "Tab", fixed: true },
  { id: "editor.cancel", get label() { return t("Cancelar la consulta en curso"); }, get group() { return t("Editor"); }, scope: "editor", default: "Escape", fixed: true },
  { id: "editor.zoomIn", get label() { return t("Agrandar la letra del SQL"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-=", fixed: true },
  { id: "editor.zoomOut", get label() { return t("Achicar la letra del SQL"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod--", fixed: true },
  { id: "editor.zoomReset", get label() { return t("Letra del SQL a su tamaño"); }, get group() { return t("Editor"); }, scope: "editor", default: "Mod-0", fixed: true },

  { id: "results.next", get label() { return t("Resultado siguiente"); }, get group() { return t("Resultados"); }, scope: "results", default: "Alt-.", fixed: true },
  { id: "results.prev", get label() { return t("Resultado anterior"); }, get group() { return t("Resultados"); }, scope: "results", default: "Alt-ArrowLeft", fixed: true },
  { id: "results.n", get label() { return t("Ir al resultado N (1 a 9)"); }, get group() { return t("Resultados"); }, scope: "results", default: "Alt-1", fixed: true },

  { id: "grid.copy", get label() { return t("Copiar el rango (TSV)"); }, get group() { return t("Grilla"); }, scope: "grid", default: "Mod-c", fixed: true },
  { id: "grid.copyCsv", get label() { return t("Copiar el rango con encabezados (CSV)"); }, get group() { return t("Grilla"); }, scope: "grid", default: "Mod-Shift-c", fixed: true },
  { id: "grid.filter", get label() { return t("Filtrar lo ya cargado"); }, get group() { return t("Grilla"); }, scope: "grid", default: "Mod-f", fixed: true },
  { id: "grid.viewer", get label() { return t("Ver el valor de la celda"); }, get group() { return t("Grilla"); }, scope: "grid", default: "Space", fixed: true },

  { id: "tree.rename", get label() { return t("Renombrar la carpeta o editar el servidor"); }, get group() { return t("Árbol y pestañas"); }, scope: "tree", default: "F2", fixed: true },
];

const MODIFIERS = new Set(["Control", "Meta", "Shift", "Alt", "AltGraph", "OS"]);

/** Teclas que se aceptan al reasignar. Las demás no son ni estables ni portables entre teclados. */
const NAMED = new Set([
  "Enter",
  "Tab",
  "Escape",
  "Space",
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Backspace",
  "Delete",
  "Home",
  "End",
  "PageUp",
  "PageDown",
]);
const SYMBOLS = new Set(["/", ",", ".", "-", "=", "[", "]", ";", "'", "\\", "`"]);

export interface ParsedCombo {
  mod: boolean;
  alt: boolean;
  shift: boolean;
  key: string;
}

function validKey(key: string): boolean {
  return (
    /^[a-z0-9]$/.test(key) || /^F([1-9]|1[0-2])$/.test(key) || NAMED.has(key) || SYMBOLS.has(key)
  );
}

export function parseCombo(text: string): ParsedCombo | null {
  // La tecla puede ser «-» (como en `Mod--`), así que se separa de atrás hacia adelante.
  const parts: string[] = [];
  let rest = text.trim();
  while (rest.length > 0) {
    const match = /^(Mod|Alt|Shift)-(.+)$/i.exec(rest);
    if (!match) break;
    parts.push(match[1].toLowerCase());
    rest = match[2];
  }
  const key = rest.length === 1 ? rest.toLowerCase() : rest;
  if (!validKey(key)) return null;
  return { mod: parts.includes("mod"), alt: parts.includes("alt"), shift: parts.includes("shift"), key };
}

export function stringifyCombo(combo: ParsedCombo): string {
  return [combo.mod ? "Mod" : "", combo.alt ? "Alt" : "", combo.shift ? "Shift" : "", combo.key]
    .filter((part) => part !== "")
    .join("-");
}

/** La forma canónica de una combinación, o `null` si no es válida. */
export function normalizeCombo(text: string): string | null {
  const parsed = parseCombo(text);
  return parsed ? stringifyCombo(parsed) : null;
}

/** La combinación que representa un evento de teclado, o `null` si es solo un modificador. */
export function comboFromEvent(event: {
  key: string;
  code?: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}): string | null {
  if (MODIFIERS.has(event.key)) return null;
  let key = event.key;
  // Con `Alt` apretada, `event.key` trae el carácter compuesto (en macOS, `Alt+B` es «∫»): la
  // posición física de la tecla es lo que sigue diciendo cuál es.
  if (event.altKey && event.code) {
    if (/^Key[A-Z]$/.test(event.code)) key = event.code[3];
    else if (/^Digit\d$/.test(event.code)) key = event.code[5];
  }
  if (key === " ") key = "Space";
  if (key === "Esc") key = "Escape";
  const parsed: ParsedCombo = {
    mod: event.ctrlKey || event.metaKey,
    alt: event.altKey,
    shift: event.shiftKey,
    key: key.length === 1 ? key.toLowerCase() : key,
  };
  return stringifyCombo(parsed);
}

export function matchesEvent(
  event: Parameters<typeof comboFromEvent>[0],
  combo: string | null,
): boolean {
  if (combo === null) return false;
  return comboFromEvent(event) === normalizeCombo(combo);
}

/**
 * Por qué una combinación no se puede asignar, o `null` si sirve. Sin `Ctrl` ni `Alt` (ni ser una
 * tecla de función) una letra suelta se comería lo que se escribe en cada campo de texto.
 */
export function validateCombo(text: string): string | null {
  const parsed = parseCombo(text);
  if (!parsed) return t("Esa tecla no se puede usar como atajo.");
  const isFunction = /^F\d+$/.test(parsed.key);
  if (!parsed.mod && !parsed.alt && !isFunction) {
    return t("Tiene que llevar Ctrl o Alt (o ser una tecla de función): una tecla suelta se comería lo que se escribe.");
  }
  if (parsed.shift && SYMBOLS.has(parsed.key)) {
    return t("Con Mayús, los símbolos cambian según el teclado: usá una letra, un número o una tecla de función.");
  }
  return null;
}

const KEY_LABEL: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Escape: "Esc",
};

/** El texto con que se muestra: `Ctrl+Mayús+F`. */
export function formatCombo(combo: string | null, mac = false): string {
  if (combo === null) return t("Sin asignar");
  const parsed = parseCombo(combo);
  if (!parsed) return combo;
  const key =
    parsed.key === "Space"
      ? t("Espacio")
      : (KEY_LABEL[parsed.key] ?? (parsed.key.length === 1 ? parsed.key.toUpperCase() : parsed.key));
  return [
    parsed.mod ? (mac ? "⌘" : "Ctrl") : "",
    parsed.alt ? (mac ? "⌥" : "Alt") : "",
    parsed.shift ? (mac ? "⇧" : t("Mayús")) : "",
    key,
  ]
    .filter((part) => part !== "")
    .join(mac ? "" : "+");
}

export type Overrides = Record<string, string | null>;

/** La combinación vigente de cada atajo: la de fábrica salvo lo reasignado. */
export function effective(overrides: Overrides, defs: readonly ShortcutDef[] = SHORTCUTS): Map<string, string | null> {
  const out = new Map<string, string | null>();
  for (const def of defs) {
    const custom = !def.fixed && def.id in overrides ? overrides[def.id] : undefined;
    out.set(def.id, custom === undefined ? def.default : custom);
  }
  return out;
}

function overlap(a: Scope, b: Scope): boolean {
  // `results` y `grid` viven dentro de la ventana, así que un atajo global los pisa y viceversa.
  return a === "global" || b === "global" || a === b;
}

/**
 * Qué atajos se pisan: para cada id, los otros que tienen la misma combinación en un contexto que
 * se solapa. Los fijos entran también —reasignar algo a `Ctrl+C` tiene que avisar—.
 */
export function findConflicts(
  bindings: ReadonlyMap<string, string | null>,
  defs: readonly ShortcutDef[] = SHORTCUTS,
): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const a of defs) {
    const ka = bindings.get(a.id);
    if (!ka) continue;
    for (const b of defs) {
      if (a.id === b.id) continue;
      const kb = bindings.get(b.id);
      if (!kb || normalizeCombo(ka) !== normalizeCombo(kb) || !overlap(a.scope, b.scope)) continue;
      // «Guardar» y «Guardar como» no se pisan con el mismo atajo del editor, que es el mismo
      // atajo: el editor los delega a esta tabla. Entre fijos que comparten tecla por diseño
      // (Alt-1 … Alt-9) tampoco hay nada que avisar.
      if (a.fixed && b.fixed) continue;
      const list = out.get(a.id) ?? [];
      list.push(b.id);
      out.set(a.id, list);
    }
  }
  return out;
}

/** Lo guardado, saneado: solo ids conocidos y reasignables, con combinaciones válidas (o `null`). */
export function parseOverrides(raw: string | null, defs: readonly ShortcutDef[] = SHORTCUTS): Overrides {
  if (!raw) return {};
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return {};
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) return {};
  const out: Overrides = {};
  for (const def of defs) {
    if (def.fixed) continue;
    const stored = (value as Record<string, unknown>)[def.id];
    if (stored === null) out[def.id] = null;
    else if (typeof stored === "string") {
      const combo = normalizeCombo(stored);
      if (combo && validateCombo(combo) === null) out[def.id] = combo;
    }
  }
  return out;
}
