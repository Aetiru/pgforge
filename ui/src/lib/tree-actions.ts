/**
 * Qué se puede abrir desde una fila del árbol.
 *
 * Vive suelto y no adentro de `DetailPanel` porque ahora hay tres puertas a lo mismo —el botón del
 * panel de detalle, el menú del clic derecho y `Ctrl+Q`—, y la regla de contra qué base abre una
 * consulta cada fila tiene que ser una sola. Es pura, así que se prueba sin montar nada.
 */

import type { ConnectionProfile, TreeNode } from "./ipc";
import type { Row } from "./explorer.svelte";

/** Contra qué base abriría una consulta o una grilla lo que está seleccionado. */
export interface QueryTarget {
  database: string;
  /** Con qué nombre aparece la pestaña. */
  title: string;
}

/** Las relaciones que tienen filas para mostrar. */
const WITH_ROWS = ["table", "partitionedTable", "view", "materializedView", "foreignTable"];

/**
 * Los objetos llevan la base encima; la fila del servidor recién conectado usa la del perfil.
 *
 * `null` cuando no hay contra qué consultar: una carpeta de conexiones, o un servidor apagado.
 */
export function queryTargetOf(
  row: Row | null,
  profile: ConnectionProfile | null | undefined,
): QueryTarget | null {
  if (!row) return null;
  if (row.node) return { database: row.node.database, title: row.node.label };
  if (row.kind === "server" && row.connected && profile) {
    return { database: profile.database, title: profile.name };
  }
  return null;
}

/** El OID de la relación cuyos datos se pueden abrir, o `null` si la fila no es una. */
export function dataTargetOf(node: TreeNode | null): number | null {
  if (!node || typeof node.kind !== "string" || !WITH_ROWS.includes(node.kind)) return null;
  return node.oid ?? null;
}

/** El esquema del que se puede dibujar un diagrama, o `null`. */
export function erdTargetOf(node: TreeNode | null): { database: string; schema: string } | null {
  if (!node || node.kind !== "schema") return null;
  return { database: node.database, schema: node.label };
}

/** El nombre completo del objeto, tal como se escribe en una consulta. */
export function qualifiedNameOf(node: TreeNode | null): string | null {
  if (!node) return null;
  return node.schema ? `${node.schema}.${node.label}` : node.label;
}
