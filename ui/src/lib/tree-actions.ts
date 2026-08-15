/**
 * Qué se puede abrir desde una fila del árbol.
 *
 * Vive suelto y no adentro de `DetailPanel` porque ahora hay tres puertas a lo mismo —el botón del
 * panel de detalle, el menú del clic derecho y `Ctrl+Q`—, y la regla de contra qué base abre una
 * consulta cada fila tiene que ser una sola. Es pura, así que se prueba sin montar nada.
 */

import type { ConnectionProfile, FolderKind, NodeKind, TreeNode } from "./ipc";
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

/**
 * El esquema sobre el que trabajan las acciones que toman un esquema entero: el diagrama y la
 * comparación contra otro servidor. `null` para cualquier otra fila.
 *
 * Es una sola función para las dos porque la pregunta es la misma —«¿esta fila es un esquema?»— y
 * dos copias se desincronizarían el día que aparezca una tercera acción de esta clase.
 */
export function schemaTargetOf(node: TreeNode | null): { database: string; schema: string } | null {
  if (!node || node.kind !== "schema") return null;
  return { database: node.database, schema: node.label };
}

/**
 * En qué carpeta del árbol vive cada tipo de objeto.
 *
 * Es lo que convierte una coincidencia de la búsqueda en un camino: sabiendo el tipo se baja
 * directo al cajón que le toca en vez de abrir los ocho del esquema. `null` para lo que no cuelga
 * de un esquema —una base, un rol— o para lo que la búsqueda no devuelve.
 */
const FOLDER_OF: Partial<Record<string, FolderKind>> = {
  table: "tables",
  partitionedTable: "tables",
  view: "views",
  materializedView: "materializedViews",
  foreignTable: "foreignTables",
  sequence: "sequences",
  function: "functions",
  procedure: "procedures",
  type: "types",
};

export function folderForKind(kind: NodeKind): FolderKind | null {
  return typeof kind === "string" ? (FOLDER_OF[kind] ?? null) : null;
}

/**
 * La cadena de conexión del perfil, para pegarla en `psql` o en otra herramienta.
 *
 * **Nunca lleva la contraseña**: vive en el almacén del sistema operativo y sacarla de ahí para
 * dejarla en el portapapeles es exactamente lo que ese almacén evita. El usuario va escapado porque
 * un rol puede llamarse `admin@casa` y ahí la arroba parte la URL en dos.
 */
export function connectionUrl(profile: ConnectionProfile): string {
  const user = encodeURIComponent(profile.user);
  const database = encodeURIComponent(profile.database);
  return `postgres://${user}@${profile.host}:${profile.port}/${database}`;
}

/** El nombre completo del objeto, tal como se escribe en una consulta. */
export function qualifiedNameOf(node: TreeNode | null): string | null {
  if (!node) return null;
  return node.schema ? `${node.schema}.${node.label}` : node.label;
}
