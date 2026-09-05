/**
 * Marcadores del árbol: objetos del catálogo que el usuario decidió tener siempre a mano.
 *
 * Espejo de `pgforge_core::bookmarks`. Los cinco comandos devuelven la lista entera —igual que
 * `snippet_*`—: es corta, así la interfaz no la recompone a mano tras cada operación puntual.
 */

import { invoke } from "./core";

export type BookmarkKind = "table" | "view" | "materializedView" | "function" | "procedure";

/** A qué objeto apunta un marcador; es la clave que lo identifica sin el `id`. */
export interface BookmarkTarget {
  profileId: string;
  database: string;
  schema: string;
  name: string;
  kind: BookmarkKind;
}

export interface Bookmark extends BookmarkTarget {
  id: number;
  /** Alias del usuario; `null` muestra el nombre del objeto tal cual. */
  label: string | null;
  /** Segundos desde el epoch. */
  createdAt: number;
}

export const bookmarksList = () => invoke<Bookmark[]>("bookmarks_list");

/** Marca un objeto. Apretar la estrella sobre algo ya marcado no falla ni duplica. */
export const bookmarkAdd = (target: BookmarkTarget, label: string | null) =>
  invoke<Bookmark[]>("bookmark_add", { target, label });

export const bookmarkRemove = (id: number) => invoke<Bookmark[]>("bookmark_remove", { id });

/** Borra por objeto, para cuando la estrella se apaga y ahí solo se tiene el objeto, no el `id`. */
export const bookmarkRemoveTarget = (target: BookmarkTarget) =>
  invoke<Bookmark[]>("bookmark_remove_target", { target });

export const bookmarkLabel = (id: number, label: string | null) =>
  invoke<Bookmark[]>("bookmark_label", { id, label });
