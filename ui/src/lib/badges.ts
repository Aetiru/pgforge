import { folderOf, type NodeKind } from "./ipc";

/**
 * Distintivo corto de cada tipo de objeto.
 *
 * Un árbol de base de datos mezcla catorce tipos de nodo; sin una marca visual, distinguir una
 * vista de una tabla obliga a leer el nombre completo de cada fila.
 */
export interface Badge {
  text: string;
  tone: string;
}

const TONES = {
  slate: "bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-200",
  blue: "bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-200",
  emerald: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-200",
  violet: "bg-violet-100 text-violet-700 dark:bg-violet-900 dark:text-violet-200",
  amber: "bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200",
  rose: "bg-rose-100 text-rose-700 dark:bg-rose-900 dark:text-rose-200",
} as const;

export function badgeFor(kind: NodeKind | null): Badge {
  if (kind === null) return { text: "SV", tone: TONES.slate };

  const folder = folderOf(kind);
  if (folder !== null) return { text: "", tone: TONES.slate };

  switch (kind) {
    case "database":
      return { text: "BD", tone: TONES.slate };
    case "schema":
      return { text: "ES", tone: TONES.slate };
    case "table":
      return { text: "TB", tone: TONES.blue };
    case "partitionedTable":
      return { text: "TP", tone: TONES.blue };
    case "foreignTable":
      return { text: "TE", tone: TONES.blue };
    case "view":
      return { text: "VI", tone: TONES.emerald };
    case "materializedView":
      return { text: "VM", tone: TONES.emerald };
    case "sequence":
      return { text: "SQ", tone: TONES.amber };
    case "function":
      return { text: "FN", tone: TONES.violet };
    case "procedure":
      return { text: "PR", tone: TONES.violet };
    case "type":
      return { text: "TY", tone: TONES.amber };
    case "column":
      return { text: "CO", tone: TONES.slate };
    case "index":
      return { text: "IX", tone: TONES.rose };
    case "constraint":
      return { text: "RS", tone: TONES.rose };
    case "trigger":
      return { text: "DP", tone: TONES.rose };
    default:
      return { text: "??", tone: TONES.slate };
  }
}

/** Nombre legible del tipo de nodo, para el panel de propiedades. */
export function kindLabel(kind: NodeKind | null): string {
  if (kind === null) return "Servidor";

  const folder = folderOf(kind);
  if (folder !== null) return "Carpeta";

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
