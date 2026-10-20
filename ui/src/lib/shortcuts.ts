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
  { id: "app.palette", label: "Paleta de comandos", group: "Aplicación", scope: "global", default: "Mod-k" },
  { id: "app.search", label: "Buscar objetos en el servidor", group: "Aplicación", scope: "global", default: "Mod-Shift-p" },
  { id: "app.newQuery", label: "Nueva consulta", group: "Aplicación", scope: "global", default: "Mod-q" },
  { id: "app.open", label: "Abrir un archivo SQL", group: "Aplicación", scope: "global", default: "Mod-o" },
  { id: "app.save", label: "Guardar la consulta", group: "Aplicación", scope: "global", default: "Mod-s" },
  { id: "app.saveAs", label: "Guardar como…", group: "Aplicación", scope: "global", default: "Mod-Shift-s" },
  { id: "app.sidebar", label: "Mostrar u ocultar el panel lateral", group: "Aplicación", scope: "global", default: "Mod-b" },
  { id: "app.inspector", label: "Mostrar u ocultar el inspector", group: "Aplicación", scope: "global", default: "Mod-i" },
  { id: "app.shortcuts", label: "Esta chuleta de atajos", group: "Aplicación", scope: "global", default: "F1" },

  { id: "editor.run", label: "Ejecutar la sentencia o la selección", group: "Editor", scope: "editor", default: "Mod-Enter" },
  { id: "editor.runScript", label: "Ejecutar el script entero", group: "Editor", scope: "editor", default: "Mod-Shift-Enter" },
  { id: "editor.format", label: "Formatear el SQL", group: "Editor", scope: "editor", default: "Mod-Shift-f" },
  { id: "editor.expand", label: "Expandir una abreviatura / saltar al hueco siguiente", group: "Editor", scope: "editor", default: "Tab", fixed: true },
  { id: "editor.cancel", label: "Cancelar la consulta en curso", group: "Editor", scope: "editor", default: "Escape", fixed: true },
  { id: "editor.zoomIn", label: "Agrandar la letra del SQL", group: "Editor", scope: "editor", default: "Mod-=", fixed: true },
  { id: "editor.zoomOut", label: "Achicar la letra del SQL", group: "Editor", scope: "editor", default: "Mod--", fixed: true },
  { id: "editor.zoomReset", label: "Letra del SQL a su tamaño", group: "Editor", scope: "editor", default: "Mod-0", fixed: true },

  { id: "results.next", label: "Resultado siguiente", group: "Resultados", scope: "results", default: "Alt-ArrowRight", fixed: true },
  { id: "results.prev", label: "Resultado anterior", group: "Resultados", scope: "results", default: "Alt-ArrowLeft", fixed: true },
  { id: "results.n", label: "Ir al resultado N (1 a 9)", group: "Resultados", scope: "results", default: "Alt-1", fixed: true },

  { id: "grid.copy", label: "Copiar el rango (TSV)", group: "Grilla", scope: "grid", default: "Mod-c", fixed: true },
  { id: "grid.copyCsv", label: "Copiar el rango con encabezados (CSV)", group: "Grilla", scope: "grid", default: "Mod-Shift-c", fixed: true },
  { id: "grid.filter", label: "Filtrar lo ya cargado", group: "Grilla", scope: "grid", default: "Mod-f", fixed: true },
  { id: "grid.viewer", label: "Ver el valor de la celda", group: "Grilla", scope: "grid", default: "Space", fixed: true },

  { id: "tree.rename", label: "Renombrar la carpeta o editar el servidor", group: "Árbol y pestañas", scope: "tree", default: "F2", fixed: true },
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
  if (!parsed) return "Esa tecla no se puede usar como atajo.";
  const isFunction = /^F\d+$/.test(parsed.key);
  if (!parsed.mod && !parsed.alt && !isFunction) {
    return "Tiene que llevar Ctrl o Alt (o ser una tecla de función): una tecla suelta se comería lo que se escribe.";
  }
  if (parsed.shift && SYMBOLS.has(parsed.key)) {
    return "Con Mayús, los símbolos cambian según el teclado: usá una letra, un número o una tecla de función.";
  }
  return null;
}

const KEY_LABEL: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Escape: "Esc",
  Space: "Espacio",
};

/** El texto con que se muestra: `Ctrl+Mayús+F`. */
export function formatCombo(combo: string | null, mac = false): string {
  if (combo === null) return "Sin asignar";
  const parsed = parseCombo(combo);
  if (!parsed) return combo;
  const key = KEY_LABEL[parsed.key] ?? (parsed.key.length === 1 ? parsed.key.toUpperCase() : parsed.key);
  return [
    parsed.mod ? (mac ? "⌘" : "Ctrl") : "",
    parsed.alt ? (mac ? "⌥" : "Alt") : "",
    parsed.shift ? (mac ? "⇧" : "Mayús") : "",
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
