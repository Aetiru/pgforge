/**
 * Esquemas que el usuario eligió no ver en el árbol, por servidor y base.
 *
 * Una base con cuarenta esquemas esconde los tres que se usan. Lo que se oculta es una preferencia
 * de esta computadora, no de la base: ocultar no cambia nada del lado del servidor. Es puro y con
 * Vitest.
 */

export type HiddenMap = Record<string, string[]>;

export function hiddenKey(profileId: string, database: string): string {
  return `${profileId}\u0001${database}`;
}

/** Lo guardado en `localStorage`, saneado. */
export function parseHidden(raw: string | null): HiddenMap {
  if (!raw) return {};
  try {
    const data: unknown = JSON.parse(raw);
    if (typeof data !== "object" || data === null || Array.isArray(data)) return {};
    const out: HiddenMap = {};
    for (const [key, list] of Object.entries(data)) {
      if (!Array.isArray(list)) continue;
      const names = [...new Set(list.filter((name): name is string => typeof name === "string"))];
      if (names.length > 0) out[key] = names;
    }
    return out;
  } catch {
    return {};
  }
}

/**
 * Qué esquemas quedan visibles. Lo que no está en `hidden` se ve: un esquema que se crea después
 * aparece solo, en vez de quedar oculto por no estar en una lista de «los que se ven».
 */
export function visibleSchemas<T extends { label: string }>(
  nodes: readonly T[],
  hidden: readonly string[],
): { shown: T[]; hiddenCount: number } {
  if (hidden.length === 0) return { shown: [...nodes], hiddenCount: 0 };
  const skip = new Set(hidden);
  const shown = nodes.filter((node) => !skip.has(node.label));
  return { shown, hiddenCount: nodes.length - shown.length };
}
