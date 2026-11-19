/**
 * La lista de abreviaturas vista por el usuario: buscar, ordenar, agrupar y decidir cuáles valen en
 * el servidor y la base en los que está escribiendo. Puro y sin DOM.
 *
 * Trabaja sobre lo mínimo que necesita de una abreviatura (`SnippetItem`), que `Snippet` de `ipc/`
 * cumple por estructura: así estas reglas no dependen de cuántos campos tenga el espejo de Rust.
 */

export interface SnippetScope {
  profileIds: string[];
  databases: string[];
}

export interface SnippetItem {
  id?: string;
  abbreviation: string;
  body: string;
  description: string;
  category?: string;
  scope?: SnippetScope;
}

/** Lo que se cuenta de cada abreviatura. `lastUsedAt` en segundos desde el epoch, como el resto. */
export interface UsageInfo {
  uses: number;
  lastUsedAt: number | null;
}

export type SnippetOrder = "name" | "uses" | "recent";

/** Sin acentos ni mayúsculas: se escribe «expresion» para encontrar «Expresión». */
const fold = (text: string) =>
  text
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();

/** Cada palabra de la consulta tiene que estar en la abreviatura, la descripción, la categoría o el texto. */
export function filterSnippets<T extends SnippetItem>(items: T[], query: string): T[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return items;
  return items.filter((item) => {
    const haystack = fold(`${item.abbreviation} ${item.description} ${item.category ?? ""} ${item.body}`);
    return words.every((word) => haystack.includes(word));
  });
}

const byName = (a: SnippetItem, b: SnippetItem) =>
  a.abbreviation.localeCompare(b.abbreviation, "es", { sensitivity: "base" });

const usageOf = (usage: Record<string, UsageInfo>, item: SnippetItem): UsageInfo | undefined =>
  item.id === undefined ? undefined : usage[item.id];

/** Una copia ordenada; el desempate es siempre el nombre, para que el orden no salte entre corridas. */
export function sortSnippets<T extends SnippetItem>(
  items: T[],
  order: SnippetOrder,
  usage: Record<string, UsageInfo> = {},
): T[] {
  const copy = [...items];
  if (order === "uses") {
    copy.sort((a, b) => (usageOf(usage, b)?.uses ?? 0) - (usageOf(usage, a)?.uses ?? 0) || byName(a, b));
  } else if (order === "recent") {
    // Las que nunca se usaron van al final.
    const at = (item: T) => usageOf(usage, item)?.lastUsedAt ?? -1;
    copy.sort((a, b) => at(b) - at(a) || byName(a, b));
  } else {
    copy.sort(byName);
  }
  return copy;
}

export const NO_CATEGORY = "Sin categoría";

/** Agrupa por categoría, en orden alfabético, con «Sin categoría» al final. Respeta el orden de cada grupo. */
export function groupByCategory<T extends SnippetItem>(items: T[]): { category: string; items: T[] }[] {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const category = item.category?.trim() || NO_CATEGORY;
    groups.set(category, [...(groups.get(category) ?? []), item]);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => {
      if (a === NO_CATEGORY) return 1;
      if (b === NO_CATEGORY) return -1;
      return a.localeCompare(b, "es", { sensitivity: "base" });
    })
    .map(([category, list]) => ({ category, items: list }));
}

/**
 * Si la abreviatura vale en este servidor y esta base. Un alcance vacío es «en todos lados»; con
 * perfiles, el actual tiene que estar en la lista; con bases, la actual también. Si no se sabe
 * dónde se está (`null`), una abreviatura acotada **no** vale: mostrar «pedidos_hoy» sin saber
 * contra qué servidor se escribe es justo lo que el alcance quería evitar.
 */
export function scopeMatches(item: SnippetItem, profileId: string | null, database: string | null): boolean {
  const scope = item.scope;
  if (!scope) return true;
  if (scope.profileIds.length > 0 && (profileId === null || !scope.profileIds.includes(profileId))) {
    return false;
  }
  if (scope.databases.length > 0 && (database === null || !scope.databases.includes(database))) {
    return false;
  }
  return true;
}

export function visibleFor<T extends SnippetItem>(
  items: T[],
  profileId: string | null,
  database: string | null,
): T[] {
  return items.filter((item) => scopeMatches(item, profileId, database));
}

/** «Todos los servidores», «Producción ventas», «base ventas» o «Producción ventas · base ventas». */
export function scopeLabel(item: SnippetItem, profileName: (id: string) => string | null): string {
  const scope = item.scope;
  const parts: string[] = [];
  if (scope && scope.profileIds.length > 0) {
    parts.push(scope.profileIds.map((id) => profileName(id) ?? "servidor desconocido").join(", "));
  }
  if (scope && scope.databases.length > 0) {
    parts.push(`${scope.databases.length > 1 ? "bases" : "base"} ${scope.databases.join(", ")}`);
  }
  return parts.length > 0 ? parts.join(" · ") : "Todos los servidores";
}

/**
 * Las que no se usan: nunca se expandieron, o hace más de `days` días que no se expanden. Es una
 * propuesta para limpiar, no una orden: quien tiene una abreviatura para el cierre de año la ve acá.
 * `nowSeconds` se pasa para poder probarlo sin reloj.
 */
export function unusedIds(
  items: SnippetItem[],
  usage: Record<string, UsageInfo>,
  days = 60,
  nowSeconds = Date.now() / 1000,
): string[] {
  const limit = nowSeconds - days * 86_400;
  const ids: string[] = [];
  for (const item of items) {
    if (item.id === undefined) continue;
    const info = usage[item.id];
    if (!info || info.uses === 0 || info.lastUsedAt === null || info.lastUsedAt < limit) ids.push(item.id);
  }
  return ids;
}
