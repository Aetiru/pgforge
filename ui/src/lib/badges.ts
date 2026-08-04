import type { IconName } from "./Icon.svelte";
import { folderOf, type NodeKind, type NodeTag } from "./ipc";

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
const SECURITY = "text-cyan-600 dark:text-cyan-400";

/**
 * Una carpeta de conexiones. No pasa por `lookOf` porque no es un nodo del catálogo: es una
 * agrupación local, y se distingue en amarillo de las carpetas grises que agrupan objetos del
 * servidor.
 */
export const GROUP_LOOK: NodeLook = { icon: "folder", tone: VALUE };

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
    case "policy":
      return { icon: "policy", tone: SECURITY };
    case "role":
      return { icon: "role", tone: SECURITY };
    default:
      return { icon: "folder", tone: STRUCTURE };
  }
}

/**
 * Texto y color de las etiquetas de un nodo.
 *
 * Son pocas y de vocabulario cerrado a propósito (`NodeTag` del núcleo): una etiqueta que se
 * reconoce por el color no se lee, y eso solo funciona si no se multiplican.
 */
const TAGS: Record<NodeTag, { label: string; tone: string; title: string }> = {
  login: {
    label: "login",
    tone: "tag-info",
    title: "El rol puede iniciar sesión",
  },
  group: {
    label: "grupo",
    tone: "tag-neutral",
    title: "El rol no puede iniciar sesión: existe para agrupar privilegios",
  },
  superuser: {
    label: "superusuario",
    tone: "tag-warn",
    title: "El rol se saltea todos los controles de permisos",
  },
  partition: {
    label: "partición",
    tone: "tag-neutral",
    title: "La tabla es una partición de otra",
  },
  rowSecurity: {
    label: "RLS",
    tone: "tag-info",
    title: "Seguridad por fila activa: la tabla puede devolver menos filas de las que tiene",
  },
};

export function tagLook(tag: NodeTag) {
  return TAGS[tag];
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
    policy: "Política",
    role: "Rol",
  };
  return names[kind as string] ?? String(kind);
}
