/**
 * Lo que se escribe en la caja de búsqueda del árbol.
 *
 * Un nombre suelto busca por nombre y nada más. Con un prefijo —`t:factura`— además se acota al tipo
 * de objeto, que es lo que hace usable la caja en un esquema donde la misma palabra aparece en la
 * tabla, en su índice, en su secuencia y en la vista que la lee. Los prefijos son una letra y dos
 * puntos porque se escriben sin pensar; el vocabulario es cerrado y lo que no está en él se toma
 * como texto, así que un nombre con dos puntos adentro sigue buscándose entero.
 */

import type { NodeKind, SearchHit } from "./ipc";

/** Qué acota cada prefijo. Una letra puede abarcar más de una clase cuando el usuario no distingue:
 * quien escribe `v:` quiere vistas, le da igual si son materializadas. */
const PREFIXES: Record<string, NodeKind[]> = {
  t: ["table", "partitionedTable", "foreignTable"],
  v: ["view", "materializedView"],
  s: ["schema"],
  f: ["function", "procedure"],
  i: ["index"],
  q: ["sequence"],
  c: ["column"],
  r: ["role"],
  y: ["type"],
};

/** Cómo se le explica al usuario qué puede escribir. */
export const PREFIX_HELP =
  "t: tablas · v: vistas · s: esquemas · f: funciones · i: índices · q: secuencias · c: columnas · r: roles · y: tipos";

export interface TreeQuery {
  /** El texto a buscar, ya sin el prefijo y sin espacios de más. */
  text: string;
  /** Las clases a las que se acota, o `null` si no se acotó a ninguna. */
  kinds: NodeKind[] | null;
}

export function parseQuery(input: string): TreeQuery {
  const trimmed = input.trim();
  const match = /^([a-zA-Z]):(.*)$/s.exec(trimmed);
  if (!match) return { text: trimmed, kinds: null };

  const kinds = PREFIXES[match[1].toLowerCase()];
  // Una letra que no es prefijo no es un error: es parte de lo que se está buscando.
  if (!kinds) return { text: trimmed, kinds: null };

  return { text: match[2].trim(), kinds };
}

/** Si la clase de un nodo entra en lo que pide la consulta. */
export function matchesKind(query: TreeQuery, kind: NodeKind | null | undefined): boolean {
  if (query.kinds === null) return true;
  if (kind === null || kind === undefined || typeof kind === "object") return false;
  return query.kinds.includes(kind);
}

/**
 * Acota el resultado de la búsqueda contra el servidor.
 *
 * Al servidor se le manda solo el texto: el prefijo es vocabulario de esta interfaz y agregarlo al
 * comando obligaría a mantener la misma tabla en los dos lados. Filtrar acá cuesta nada —el
 * resultado ya viene con tope— y deja al núcleo sin enterarse.
 */
export function filterHits(query: TreeQuery, hits: SearchHit[]): SearchHit[] {
  if (query.kinds === null) return hits;
  return hits.filter((hit) => matchesKind(query, hit.kind));
}
