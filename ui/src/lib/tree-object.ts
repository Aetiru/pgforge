/**
 * Qué objeto del catálogo es una fila del árbol, en términos que sirven para escribir SQL: su nombre
 * calificado y de qué familia es. Lo usan arrastrar al editor, los botones de la fila y la vista
 * rápida; el árbol mismo no lo necesita. Es puro y con Vitest.
 */
import type { TreeNode } from "./ipc";
import { sqlIdent } from "./sql-generate";

export type ObjectFamily = "relation" | "routine" | "sequence";

export interface ObjectRef {
  family: ObjectFamily;
  schema: string;
  /** El nombre solo: sin los argumentos de una función. */
  name: string;
  database: string;
  oid: number;
  /** `esquema.nombre`, citado donde hace falta: lo que se pega en una consulta. */
  qualified: string;
  /** Una relación cuyas filas se pueden ver (tabla, vista, vista materializada, foránea). */
  hasRows: boolean;
}

const RELATIONS = new Set(["table", "partitionedTable", "foreignTable", "view", "materializedView"]);

export function objectRef(node: TreeNode | null | undefined): ObjectRef | null {
  if (!node || typeof node.kind === "object" || !node.schema || node.oid === undefined) return null;

  const family: ObjectFamily | null = RELATIONS.has(node.kind)
    ? "relation"
    : node.kind === "function" || node.kind === "procedure"
      ? "routine"
      : node.kind === "sequence"
        ? "sequence"
        : null;
  if (!family) return null;

  // La etiqueta de una función lleva la firma: `calcular(a integer)`. Se llama solo `calcular`.
  const name = family === "routine" ? node.label.replace(/\(.*$/s, "") : node.label;
  return {
    family,
    schema: node.schema,
    name,
    database: node.database,
    oid: node.oid,
    qualified: `${sqlIdent(node.schema)}.${sqlIdent(name)}`,
    hasRows: family === "relation",
  };
}

/** El tipo con el que viaja un objeto arrastrado del árbol: lo lee el editor para el `Mayús`+soltar. */
export const OBJECT_MIME = "application/x-pgforge-object";

export interface DraggedObject {
  profileId: string;
  database: string;
  oid: number;
  qualified: string;
  hasRows: boolean;
}

/** Lo que el editor lee de lo arrastrado, o `null` si no es un objeto de pgForge. */
export function parseDragged(raw: string | undefined): DraggedObject | null {
  if (!raw) return null;
  try {
    const data = JSON.parse(raw) as Partial<DraggedObject>;
    return typeof data.profileId === "string" &&
      typeof data.database === "string" &&
      typeof data.oid === "number" &&
      typeof data.qualified === "string"
      ? { profileId: data.profileId, database: data.database, oid: data.oid, qualified: data.qualified, hasRows: data.hasRows === true }
      : null;
  } catch {
    return null;
  }
}
