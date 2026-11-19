/**
 * Vistas guardadas de la grilla de datos: una combinación de columnas visibles, orden y filtro, con
 * nombre, por tabla («Pendientes de hoy»). Es lo que uno vuelve a armar cada vez que abre una tabla
 * grande. Viven en esta computadora y se identifican por servidor, base, esquema y tabla, no por oid.
 * Es puro y con Vitest.
 */
import type { PageOrder } from "./ipc";

export interface GridView {
  name: string;
  /** Nombres de las columnas escondidas (no sus claves de grilla, que llevan la posición). */
  hidden: string[];
  order: PageOrder | null;
  filter: string;
}

export type ViewMap = Record<string, GridView[]>;

export const MAX_VIEWS_PER_TABLE = 20;
export const MAX_NAME = 40;

export function viewsKey(profileId: string, database: string, schema: string, table: string): string {
  return [profileId, database, schema, table].join("\u0001");
}

/** El nombre de la columna dentro de la clave de la grilla (`3-nombre` → `nombre`). */
export function columnNameOfKey(key: string): string {
  return key.replace(/^\d+-/, "");
}

/** Agrega o reemplaza (mismo nombre, sin distinguir mayúsculas) y respeta el tope. */
export function withView(views: readonly GridView[], view: GridView): GridView[] {
  const name = view.name.trim().slice(0, MAX_NAME);
  if (name === "") return [...views];
  const next = views.filter((item) => item.name.toLowerCase() !== name.toLowerCase());
  next.push({ ...view, name });
  return next.slice(-MAX_VIEWS_PER_TABLE);
}

export function withoutView(views: readonly GridView[], name: string): GridView[] {
  return views.filter((item) => item.name !== name);
}

function parseView(value: unknown): GridView | null {
  if (typeof value !== "object" || value === null) return null;
  const { name, hidden, order, filter } = value as Record<string, unknown>;
  if (typeof name !== "string" || name.trim() === "") return null;
  const parsedOrder =
    typeof order === "object" && order !== null &&
    typeof (order as PageOrder).column === "string" && typeof (order as PageOrder).descending === "boolean"
      ? { column: (order as PageOrder).column, descending: (order as PageOrder).descending }
      : null;
  return {
    name: name.trim().slice(0, MAX_NAME),
    hidden: Array.isArray(hidden) ? hidden.filter((h): h is string => typeof h === "string") : [],
    order: parsedOrder,
    filter: typeof filter === "string" ? filter : "",
  };
}

/** Lo guardado en `localStorage`, saneado. */
export function parseViews(raw: string | null): ViewMap {
  if (!raw) return {};
  try {
    const data: unknown = JSON.parse(raw);
    if (typeof data !== "object" || data === null || Array.isArray(data)) return {};
    const out: ViewMap = {};
    for (const [key, list] of Object.entries(data)) {
      if (!Array.isArray(list)) continue;
      const views = list.map(parseView).filter((v): v is GridView => v !== null);
      if (views.length > 0) out[key] = views.slice(0, MAX_VIEWS_PER_TABLE);
    }
    return out;
  } catch {
    return {};
  }
}
