/**
 * Orden manual de los servidores en el árbol, por carpeta.
 *
 * `order` es una posición local a la carpeta del perfil y no una prioridad global: mover un
 * servidor de una carpeta a otra no tiene por qué desordenar la que dejó ni la que recibe. Sin
 * `order` (perfil viejo, o uno que nunca se arrastró) el servidor cae al orden alfabético — es lo
 * que ya hacía el árbol antes de esto, así que no romper eso es justo el punto de la comparación.
 */

import type { ConnectionProfile } from "./ipc";

/** Mismo criterio que usa el árbol para comparar nombres: sin distinguir mayúsculas ni acentos. */
const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

/**
 * Compara dos perfiles para el orden del árbol: primero por `order` (ausente = al final), después
 * por nombre. Con `order` ausente en los dos, el resultado es el alfabético de siempre.
 */
export function compareServers(a: ConnectionProfile, b: ConnectionProfile): number {
  const orderA = a.order ?? Number.MAX_SAFE_INTEGER;
  const orderB = b.order ?? Number.MAX_SAFE_INTEGER;
  if (orderA !== orderB) return orderA - orderB;
  return byName(a.name, b.name);
}

/**
 * Arma los parches de `order`/`group` que deja soltar `draggedId` en `targetGroup`, justo antes de
 * `beforeId` (o al final si `beforeId` es `null` o no está entre los hermanos del destino).
 *
 * Pura a propósito: quien la llama (`explorer.reorder`) es lo único que sabe guardar un perfil, y
 * separar el cálculo de la escritura es lo que la hace verificable sin tocar `saveProfile`.
 *
 * Solo devuelve parches para los hermanos del grupo destino más el arrastrado — nunca toca el
 * `order` de perfiles de otras carpetas, que ni entran en el cálculo.
 */
export function reorderDrop(
  profiles: ConnectionProfile[],
  draggedId: string,
  targetGroup: string | null,
  beforeId: string | null,
): { id: string; group: string | null; order: number }[] {
  if (draggedId === beforeId) return [];

  const dragged = profiles.find((profile) => profile.id === draggedId);
  if (!dragged) return [];

  const siblings = profiles
    .filter((profile) => (profile.group ?? null) === targetGroup && profile.id !== draggedId)
    .sort(compareServers);

  const at = beforeId === null ? -1 : siblings.findIndex((profile) => profile.id === beforeId);
  const final =
    at < 0
      ? [...siblings, dragged]
      : [...siblings.slice(0, at), dragged, ...siblings.slice(at)];

  return final.map((profile, index) => ({ id: profile.id, group: targetGroup, order: index }));
}
