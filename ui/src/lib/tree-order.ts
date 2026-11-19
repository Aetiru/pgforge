/**
 * Cómo se ordenan y agrupan las relaciones de una carpeta del árbol.
 *
 * Doscientas tablas por orden alfabético mezclan `dim_`, `fact_` y `tmp_`. Acá se decide el orden
 * (por nombre, por tamaño o por filas) y qué prefijos forman un grupo plegable; lo que dibuja cada
 * grupo es del árbol. Es puro y con Vitest.
 */
import type { TreeNode } from "./ipc";

export type OrderMode = "name" | "size" | "rows";

export const ORDER_MODES: { value: OrderMode; label: string }[] = [
  { value: "name", label: "Por nombre" },
  { value: "size", label: "Por tamaño en disco" },
  { value: "rows", label: "Por filas" },
];

const byName = (a: TreeNode, b: TreeNode) =>
  a.label.localeCompare(b.label, undefined, { sensitivity: "base" });

/**
 * Las relaciones en el orden pedido. Por tamaño o por filas van **de mayor a menor** —lo que se busca
 * es lo que pesa—, y lo que no tiene dato (el servidor no calculó el tamaño de un esquema enorme, o
 * la tabla nunca se analizó) va al final por nombre, no mezclado entre los ceros.
 */
export function orderNodes(nodes: readonly TreeNode[], mode: OrderMode): TreeNode[] {
  const copy = [...nodes];
  if (mode === "name") return copy.sort(byName);

  const measure = (node: TreeNode) => (mode === "size" ? node.sizeBytes : node.rows);
  return copy.sort((a, b) => {
    const left = measure(a);
    const right = measure(b);
    if (left === undefined && right === undefined) return byName(a, b);
    if (left === undefined) return 1;
    if (right === undefined) return -1;
    return right - left || byName(a, b);
  });
}

export interface PrefixGroup {
  prefix: string;
  nodes: TreeNode[];
}

/** Cuántas relaciones tiene que compartir un prefijo para que valga un grupo. */
export const MIN_GROUP = 3;

/** El prefijo de un nombre: hasta el primer `_`, que no puede ser el primer carácter. */
export function prefixOf(name: string): string | null {
  const at = name.indexOf("_");
  return at >= 1 && at < name.length - 1 ? name.slice(0, at + 1) : null;
}

/**
 * Parte la lista en grupos por prefijo y el resto suelto, respetando el orden en que llegó cada uno.
 * Los grupos salen por orden alfabético de prefijo y los sueltos al final. `null` si no se forma
 * ningún grupo: agrupar en un solo grupo del tamaño de la lista entera no ordena nada.
 */
export function groupByPrefix(
  nodes: readonly TreeNode[],
  min = MIN_GROUP,
): { groups: PrefixGroup[]; loose: TreeNode[] } | null {
  const byPrefix = new Map<string, TreeNode[]>();
  for (const node of nodes) {
    const prefix = prefixOf(node.label);
    if (prefix === null) continue;
    byPrefix.set(prefix, [...(byPrefix.get(prefix) ?? []), node]);
  }

  const kept = [...byPrefix.entries()]
    .filter(([, members]) => members.length >= min)
    .sort(([a], [b]) => a.localeCompare(b, undefined, { sensitivity: "base" }));
  if (kept.length === 0) return null;

  const grouped = new Set(kept.flatMap(([, members]) => members));
  const loose = nodes.filter((node) => !grouped.has(node));
  if (loose.length === 0 && kept.length === 1) return null;

  return { groups: kept.map(([prefix, members]) => ({ prefix, nodes: members })), loose };
}
