import type { IconName } from "./Icon.svelte";
import { folderOf, type NodeKind } from "./ipc";

/**
 * Ícono y color de cada tipo de nodo.
 *
 * Un árbol de base de datos mezcla quince tipos de objeto. El color agrupa por familia —relaciones
 * en azul, código en violeta, integridad en rosa— para que el ojo encuentre lo que busca sin leer
 * cada fila.
 */
export interface NodeLook {
  icon: IconName;
  tone: string;
}

const RELATION = "text-blue-600 dark:text-blue-400";
const DERIVED = "text-emerald-600 dark:text-emerald-400";
const CODE = "text-violet-600 dark:text-violet-400";
const INTEGRITY = "text-rose-500 dark:text-rose-400";
const VALUE = "text-amber-600 dark:text-amber-400";
const STRUCTURE = "text-zinc-400 dark:text-zinc-500";

export function lookOf(kind: NodeKind | null): NodeLook {
  if (kind === null) return { icon: "server", tone: STRUCTURE };
  if (folderOf(kind) !== null) return { icon: "folder", tone: STRUCTURE };

  switch (kind) {
    case "database":
      return { icon: "database", tone: STRUCTURE };
    case "schema":
      return { icon: "schema", tone: STRUCTURE };
    case "table":
      return { icon: "table", tone: RELATION };
    case "partitionedTable":
      return { icon: "partitioned", tone: RELATION };
    case "foreignTable":
      return { icon: "table", tone: RELATION };
    case "view":
      return { icon: "view", tone: DERIVED };
    case "materializedView":
      return { icon: "matview", tone: DERIVED };
    case "sequence":
      return { icon: "sequence", tone: VALUE };
    case "function":
      return { icon: "function", tone: CODE };
    case "procedure":
      return { icon: "function", tone: CODE };
    case "type":
      return { icon: "type", tone: VALUE };
    case "column":
      return { icon: "column", tone: STRUCTURE };
    case "index":
      return { icon: "index", tone: INTEGRITY };
    case "constraint":
      return { icon: "constraint", tone: INTEGRITY };
    case "trigger":
      return { icon: "trigger", tone: CODE };
    default:
      return { icon: "folder", tone: STRUCTURE };
  }
}

/** Nombre legible del tipo de nodo, para el panel de propiedades. */
export function kindLabel(kind: NodeKind | null): string {
  if (kind === null) return "Servidor";
  if (folderOf(kind) !== null) return "Carpeta";

  const names: Record<string, string> = {
    database: "Base de datos",
    schema: "Esquema",
    table: "Tabla",
    partitionedTable: "Tabla particionada",
    foreignTable: "Tabla externa",
    view: "Vista",
    materializedView: "Vista materializada",
    sequence: "Secuencia",
    function: "Función",
    procedure: "Procedimiento",
    type: "Tipo",
    column: "Columna",
    index: "Índice",
    constraint: "Restricción",
    trigger: "Disparador",
  };
  return names[kind as string] ?? String(kind);
}
