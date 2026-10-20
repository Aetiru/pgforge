/**
 * El árbol plegable de un valor JSON/JSONB para el visor de celda.
 *
 * `flatten` aplana el árbol a una lista de filas según qué nodos están abiertos, de modo que el
 * componente dibuja con un `{#each}` simple y sin recursión de componentes (un jsonb de miles de
 * claves haría un componente por nodo). Las rutas se escriben como `jsonpath` de PostgreSQL
 * (`$.a[0]."b c"`), que se pega tal cual en `jsonb_path_query`.
 *
 * Es puro y con Vitest. `JSON.parse` pierde precisión en enteros de más de 2^53; el visor es para
 * leer la estructura, y «Copiar» del diálogo sigue copiando el texto original.
 */

export type JsonKind = "object" | "array" | "string" | "number" | "boolean" | "null";

export interface JsonRow {
  /** Ruta `jsonpath` del nodo; es también su identidad para plegar y desplegar. */
  path: string;
  depth: number;
  /** Clave dentro del padre (nombre o posición); `null` en la raíz. */
  key: string | null;
  kind: JsonKind;
  /** Para un escalar su valor escrito; para un contenedor «{3}» / «[3]». */
  text: string;
  /** Cuántos hijos tiene un contenedor. */
  size: number;
  open: boolean;
}

/** Parsea y dice si es un contenedor (lo que vale la pena mostrar como árbol). */
export function parseJsonTree(text: string): { ok: true; value: unknown } | { ok: false } {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return { ok: false };
  try {
    return { ok: true, value: JSON.parse(trimmed) };
  } catch {
    return { ok: false };
  }
}

export function kindOf(value: unknown): JsonKind {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  switch (typeof value) {
    case "object":
      return "object";
    case "string":
      return "string";
    case "number":
      return "number";
    default:
      return "boolean";
  }
}

const IDENT = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** Un tramo de ruta: `.nombre`, `."nombre raro"` o `[3]`. */
export function childPath(parent: string, key: string | number): string {
  if (typeof key === "number") return `${parent}[${key}]`;
  return IDENT.test(key) ? `${parent}.${key}` : `${parent}.${JSON.stringify(key)}`;
}

function children(value: unknown): [string | number, unknown][] {
  if (Array.isArray(value)) return value.map((item, index) => [index, item]);
  if (value !== null && typeof value === "object") return Object.entries(value);
  return [];
}

function scalarText(value: unknown, kind: JsonKind): string {
  if (kind === "string") return JSON.stringify(value);
  return String(value);
}

/**
 * Las filas visibles. `open` tiene las rutas desplegadas; `maxChildren` corta los contenedores
 * enormes (el resto se resume en una fila «… N más») para que abrir un arreglo de cien mil
 * elementos no cuelgue la ventana.
 */
export function flatten(
  root: unknown,
  open: ReadonlySet<string>,
  maxChildren = 200,
): (JsonRow | { path: string; depth: number; more: number })[] {
  const out: (JsonRow | { path: string; depth: number; more: number })[] = [];

  const visit = (value: unknown, path: string, key: string | null, depth: number) => {
    const kind = kindOf(value);
    const kids = kind === "object" || kind === "array" ? children(value) : [];
    const container = kind === "object" || kind === "array";
    const isOpen = container && open.has(path);
    out.push({
      path,
      depth,
      key,
      kind,
      text: container ? (kind === "array" ? `[${kids.length}]` : `{${kids.length}}`) : scalarText(value, kind),
      size: kids.length,
      open: isOpen,
    });
    if (!isOpen) return;
    kids.slice(0, maxChildren).forEach(([k, v]) => visit(v, childPath(path, k), String(k), depth + 1));
    if (kids.length > maxChildren) {
      out.push({ path: `${path}…`, depth: depth + 1, more: kids.length - maxChildren });
    }
  };

  visit(root, "$", null, 0);
  return out;
}

/** Las rutas desplegadas por omisión: la raíz y el primer nivel. */
export function defaultOpen(root: unknown, levels = 1): Set<string> {
  const open = new Set<string>();
  const walk = (value: unknown, path: string, depth: number) => {
    const kind = kindOf(value);
    if ((kind !== "object" && kind !== "array") || depth > levels) return;
    open.add(path);
    for (const [k, v] of children(value)) walk(v, childPath(path, k), depth + 1);
  };
  walk(root, "$", 0);
  return open;
}

/** Todas las rutas de contenedores, para «Desplegar todo» (con tope: no es para valores gigantes). */
export function allContainers(root: unknown, limit = 5000): Set<string> {
  const open = new Set<string>();
  const walk = (value: unknown, path: string) => {
    if (open.size >= limit) return;
    const kind = kindOf(value);
    if (kind !== "object" && kind !== "array") return;
    open.add(path);
    for (const [k, v] of children(value)) walk(v, childPath(path, k));
  };
  walk(root, "$");
  return open;
}
