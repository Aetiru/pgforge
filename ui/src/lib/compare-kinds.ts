/**
 * Qué clase de cosa es cada fila del informe de comparación: su ícono, su nombre y a cuál de las
 * tres categorías pertenece.
 *
 * `badges.ts` resuelve los objetos que también están en el árbol (`compareLook`/`compareLabel`);
 * lo que solo existe en la comparación —los permisos de un objeto, el encendido de la seguridad
 * por filas— no tiene nodo en el árbol y se resuelve acá. Pura y con Vitest.
 */

import type { NodeLook } from "./badges";
import { compareLabel, compareLook, kindLabel, lookOf } from "./badges";
import type { CompareKind, CompareObject, DiffEntry } from "./ipc";

/** Las tres categorías en que se agrupa el informe, en el orden en que se aplican. */
export type CompareCategory = "structure" | "logic" | "access";

export const COMPARE_CATEGORIES: { key: CompareCategory; label: string; title: string }[] = [
  {
    key: "structure",
    label: "Estructura",
    title: "Tablas, vistas, secuencias y tipos",
  },
  {
    key: "logic",
    label: "Lógica",
    title: "Funciones, procedimientos y disparadores",
  },
  {
    key: "access",
    label: "Acceso",
    title: "Seguridad por filas, políticas y permisos (GRANT)",
  },
];

/** Lo que se compara, en una frase para el diálogo. */
export const COMPARE_SCOPE = [
  "tablas con sus columnas, restricciones e índices",
  "vistas",
  "secuencias",
  "tipos",
  "funciones y procedimientos",
  "disparadores",
  "políticas de seguridad por filas",
  "permisos",
];

export function categoryOf(kind: CompareKind): CompareCategory {
  switch (kind) {
    case "function":
    case "procedure":
    case "trigger":
      return "logic";
    case "policy":
    case "rowSecurity":
    case "grant":
      return "access";
    default:
      return "structure";
  }
}

/** Cuántas entradas hay de cada categoría, para los conteos de las fichas. */
export function countCategories(entries: DiffEntry[]): Record<CompareCategory, number> {
  const counts: Record<CompareCategory, number> = { structure: 0, logic: 0, access: 0 };
  for (const entry of entries) counts[categoryOf(entry.kind)] += 1;
  return counts;
}

/** Las entradas de las categorías que están encendidas. */
export function filterByCategory(
  entries: DiffEntry[],
  shown: Record<CompareCategory, boolean>,
): DiffEntry[] {
  return entries.filter((entry) => shown[categoryOf(entry.kind)]);
}

export function compareKindLook(kind: CompareKind): NodeLook {
  switch (kind) {
    case "function":
    case "procedure":
    case "trigger":
    case "policy":
      return lookOf(kind);
    // Mismo color que las políticas —es la misma familia, la del acceso—, con el ícono de lo que son.
    case "rowSecurity":
      return { icon: "lock", tone: lookOf("policy").tone };
    case "grant":
      return { icon: "key", tone: lookOf("policy").tone };
    default:
      return compareLook(kind satisfies CompareObject);
  }
}

export function compareKindLabel(kind: CompareKind): string {
  switch (kind) {
    case "function":
    case "procedure":
    case "trigger":
    case "policy":
      return kindLabel(kind);
    case "rowSecurity":
      return "Seguridad por filas";
    case "grant":
      return "Permisos";
    default:
      return compareLabel(kind satisfies CompareObject);
  }
}
