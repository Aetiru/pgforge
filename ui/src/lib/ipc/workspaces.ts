/**
 * Espacios de trabajo: ventanas propias acotadas a una carpeta de servidores.
 *
 * El modelo entero vive en `pgforge_core::workspace` (ver su comentario) — acá solo los tipos espejo
 * y una función por comando, como en el resto de `ipc/`.
 */

import { invoke } from "./core";

export type WorkspaceId = string;

/** De dónde salió el workspace: puramente informativo, no cambia cómo se administra después. */
export type WorkspaceSource = { kind: "native" } | { kind: "dbeaver"; root: string };

/**
 * Una ventana de escritorio acotada a un subárbol de carpetas de servidores.
 *
 * `rootGroup` ausente es «todo el árbol, sin acotar»; con valor, filtra a esa carpeta y las que
 * cuelgan de ella (mismo criterio que `group_starts_with` del núcleo).
 */
export interface Workspace {
  id: WorkspaceId;
  name: string;
  /** Segundos desde el epoch. */
  createdAt: number;
  rootGroup?: string;
  source: WorkspaceSource;
}

export const workspaceList = () => invoke<Workspace[]>("workspace_list");

export const workspaceGet = (id: WorkspaceId) => invoke<Workspace>("workspace_get", { id });

/** Crea un workspace nativo, acotado a `rootGroup` o a todo el árbol si no se pasa ninguno. */
export const workspaceCreate = (name: string, rootGroup?: string) =>
  invoke<Workspace>("workspace_create", { name, rootGroup: rootGroup ?? null });

export const workspaceRename = (id: WorkspaceId, name: string) =>
  invoke<void>("workspace_rename", { id, name });

export const workspaceDelete = (id: WorkspaceId) => invoke<void>("workspace_delete", { id });

/** Abre la ventana del workspace, o le da foco si ya estaba abierta. */
export const workspaceOpen = (id: WorkspaceId) => invoke<void>("workspace_open", { id });
