/**
 * Cómo se agrupan los roles en el árbol y qué marcas se les ven.
 *
 * Es lógica pura y vive acá y no en el árbol porque equivocarse se nota en la lista entera: un rol
 * que cae en el grupo que no es, o una marca que no sale, no falla ninguna prueba de compilación.
 * Los datos salen de las etiquetas que ya trae cada nodo (`login`, `superuser`, `expired`…): el
 * núcleo las calcula al leer `pg_roles`, así que agrupar no cuesta ninguna consulta más.
 */

import type { NodeTag, TreeNode } from "./ipc";

export type RoleGroupId = "users" | "groups" | "system";

/** Los tres cajones en el orden en que se miran: primero las personas, al final lo interno. */
export const ROLE_GROUP_ORDER: readonly RoleGroupId[] = ["users", "groups", "system"];

/**
 * Los internos de Postgres (`pg_read_all_data`, `pg_signal_backend`…) van aparte aunque no inicien
 * sesión: mezclados con los grupos propios, los de la aplicación quedan enterrados entre veinte.
 * El prefijo `pg_` está reservado por el servidor, que no deja crear uno con ese nombre.
 */
export function roleGroupOf(node: Pick<TreeNode, "label" | "tags">): RoleGroupId {
  if (node.label.startsWith("pg_")) return "system";
  return node.tags?.includes("login") ? "users" : "groups";
}

/** Reparte los roles entre los tres cajones, sin los que quedan vacíos y respetando el orden de llegada. */
export function groupRoles<T extends Pick<TreeNode, "label" | "tags">>(
  nodes: readonly T[],
): { id: RoleGroupId; nodes: T[] }[] {
  const by = new Map<RoleGroupId, T[]>(ROLE_GROUP_ORDER.map((id) => [id, []]));
  for (const node of nodes) by.get(roleGroupOf(node))!.push(node);
  return ROLE_GROUP_ORDER.map((id) => ({ id, nodes: by.get(id)! })).filter((group) => group.nodes.length > 0);
}

/** Solo el de sistema arranca plegado: es lo que casi nunca se busca. */
export function roleGroupOpenByDefault(id: RoleGroupId): boolean {
  return id !== "system";
}

/**
 * Las etiquetas que se dibujan junto al nombre de un rol.
 *
 * `login` y `group` se omiten: dentro de un cajón que ya dice «Usuarios» o «Grupos» repetirlo en cada
 * fila es ruido, y el que deja de lucir es el que importa (superusuario, vencido).
 */
export function roleRowTags(tags: readonly NodeTag[] | undefined): NodeTag[] {
  return (tags ?? []).filter((tag) => tag !== "login" && tag !== "group");
}
