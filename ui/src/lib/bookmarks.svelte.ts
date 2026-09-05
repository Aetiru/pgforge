/**
 * Marcadores del árbol, del lado de la interfaz: objetos del catálogo que el usuario decidió tener
 * siempre a mano.
 *
 * A diferencia de `folders.svelte.ts` (que es preferencia de esta máquina, en `localStorage`), esto
 * es un espejo de lo que guarda Rust en `bookmarks.db` —el mismo molde que `snippets.svelte.ts`—:
 * se carga una vez al arrancar (`App.svelte` la llama junto con `snippets.load()`, mismo precedente)
 * y cada operación devuelve la lista entera, así que la copia local se repone con lo que contestó el
 * servidor y no con lo que la pantalla creía tener.
 */

import {
  bookmarkAdd,
  bookmarkLabel,
  bookmarkRemove,
  bookmarkRemoveTarget,
  bookmarksList,
  describeError,
  treeSearch,
  type Bookmark,
  type BookmarkKind,
  type BookmarkTarget,
  type NodeKind,
} from "./ipc";
import { explorer, type Row } from "./explorer.svelte";
import { folderForKind } from "./tree-actions";

/** Si dos objetivos son el mismo marcador: mismo servidor, base, esquema, nombre y tipo. */
function sameTarget(a: BookmarkTarget, b: BookmarkTarget): boolean {
  return (
    a.profileId === b.profileId &&
    a.database === b.database &&
    a.schema === b.schema &&
    a.name === b.name &&
    a.kind === b.kind
  );
}

class Bookmarks {
  entries = $state<Bookmark[]>([]);
  /** Lo último que falló, para que la lista lo muestre. */
  error = $state<string | null>(null);
  private started = false;

  /**
   * Trae la lista si todavía no se trajo. Mismo criterio que `Snippets.load`: que falle no se
   * muestra con un cartel al arrancar, queda en `error` para quien la mire.
   */
  async load() {
    if (this.started) return;
    this.started = true;
    try {
      this.entries = await bookmarksList();
    } catch (failure) {
      this.error = describeError(failure);
    }
  }

  has(target: BookmarkTarget): boolean {
    return this.entries.some((entry) => sameTarget(entry, target));
  }

  /** Marca o desmarca, según ya esté o no. Es lo que enciende y apaga la estrella. */
  async toggle(target: BookmarkTarget) {
    try {
      this.entries = this.has(target)
        ? await bookmarkRemoveTarget(target)
        : await bookmarkAdd(target, null);
      this.error = null;
    } catch (failure) {
      this.error = describeError(failure);
    }
  }

  async remove(id: number) {
    try {
      this.entries = await bookmarkRemove(id);
      this.error = null;
    } catch (failure) {
      this.error = describeError(failure);
    }
  }

  async setLabel(id: number, label: string | null) {
    try {
      this.entries = await bookmarkLabel(id, label);
      this.error = null;
    } catch (failure) {
      this.error = describeError(failure);
    }
  }

  /** Los de un servidor, en el orden que ya trae `entries` (por servidor, esquema y nombre). */
  byProfile(profileId: string): Bookmark[] {
    return this.entries.filter((entry) => entry.profileId === profileId);
  }
}

export const bookmarks = new Bookmarks();

/** Si el tipo de un nodo del árbol es el que corresponde a la clase de marcador. */
function matchesKind(nodeKind: NodeKind, kind: BookmarkKind): boolean {
  if (typeof nodeKind !== "string") return false;
  // Una tabla marcada puede haberse vuelto una tabla particionada sin que el marcador lo sepa: el
  // marcador no distingue las dos, así que el camino de vuelta tampoco tiene por qué.
  if (kind === "table") return nodeKind === "table" || nodeKind === "partitionedTable";
  return nodeKind === kind;
}

/**
 * Revela un marcador en el árbol.
 *
 * El marcador no guarda el OID del objeto —solo esquema, nombre y tipo—, así que no alcanza con
 * `explorer.revealHit` (que además depende de `hitProfile`, privado y atado a una búsqueda ya en
 * curso vía la caja del árbol): se busca el nombre exacto contra el catálogo del servidor
 * (`tree_search`, el mismo comando de la búsqueda) y, entre las coincidencias, se toma la que calza
 * en esquema, nombre y tipo; con eso ya se tiene el OID y se sigue el mismo camino que una
 * coincidencia de la búsqueda (`explorer.revealObject`, `folderForKind`).
 *
 * `null` cuando el objeto ya no está —lo borraron, lo renombraron—: el marcador no se borra solo
 * (ver `pgforge_core::bookmarks`), así que quien llama tiene que poder avisar sin tocarlo.
 */
export async function revealBookmark(bookmark: Bookmark): Promise<Row | null> {
  const hits = await treeSearch(bookmark.profileId, bookmark.database, bookmark.name, explorer.options);
  const hit = hits.find(
    (item) =>
      item.schema === bookmark.schema &&
      item.label === bookmark.name &&
      matchesKind(item.kind, bookmark.kind),
  );
  const folder = hit && folderForKind(hit.kind);
  if (!hit || !folder) return null;

  return explorer.revealObject(bookmark.profileId, bookmark.database, bookmark.schema, hit.oid, folder);
}
