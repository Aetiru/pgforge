import type { IconName } from "./Icon.svelte";
import {
  folderOf,
  type CompareObject,
  type Environment,
  type NodeKind,
  type NodeTag,
} from "./ipc";

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
    case "extension":
      return { icon: "extension", tone: CODE };
    case "foreignDataWrapper":
      return { icon: "plug", tone: SECURITY };
    case "foreignServer":
      return { icon: "server", tone: SECURITY };
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

/**
 * Entorno de un servidor.
 *
 * Va aparte de `TAGS`, que es el vocabulario cerrado de `NodeTag` y describe nodos del catálogo: el
 * entorno es del perfil local y una fila de servidor no trae nodo. El rojo de producción es el punto
 * de todo esto, así que no se comparte con ningún otro estado.
 *
 * `spine` es el color de la línea que el árbol pinta en el borde izquierdo de un servidor y de todo
 * lo que cuelga de él: con veinte conexiones abiertas, saber a qué servidor pertenece la fila que se
 * está mirando no puede depender de subir con la vista hasta la raíz. Es un fondo y no un texto, así
 * que no puede salir de `tone`, que son las clases de la pastilla.
 *
 * `bar` es lo mismo para la barra de una pestaña de consulta o de datos: el borde izquierdo con el
 * color del entorno y un fondo apenas teñido. La pastilla sola no alcanzaba —vive en la otra punta
 * de la barra, entre otras dos, y con la pestaña ya abierta uno mira los botones y no el rótulo—,
 * y el botón que se está por apretar es justo lo que hay que pintar. El tinte es translúcido a
 * propósito: se lee sobre el fondo de los dos temas y no compite con lo que está escrito encima.
 */
const ENVIRONMENTS: Record<
  Environment,
  { label: string; tone: string; spine: string; bar: string; title: string }
> = {
  dev: {
    label: "dev",
    tone: "tag-ok",
    spine: "bg-emerald-500/70",
    bar: "border-l-[3px] border-l-emerald-500/70 bg-emerald-500/[0.06]",
    title: "Servidor de desarrollo",
  },
  test: {
    label: "test",
    tone: "tag-info",
    spine: "bg-blue-500/70",
    bar: "border-l-[3px] border-l-blue-500/70 bg-blue-500/[0.06]",
    title: "Servidor de pruebas",
  },
  prod: {
    label: "prod",
    tone: "tag-bad",
    // Producción pinta más fuerte que las otras dos, y además conserva la pastilla: el color solo
    // no se le puede confiar a la única distinción que importa de verdad.
    spine: "bg-rose-500/80",
    bar: "border-l-[3px] border-l-rose-500/80 bg-rose-500/[0.10]",
    title: "Servidor de producción: cada modificación pide una confirmación de más",
  },
};

export function envLook(environment: Environment) {
  return ENVIRONMENTS[environment];
}

/**
 * Las clases de la barra de una pestaña según el entorno del servidor, o el hueco equivalente
 * cuando no está marcado: sin el borde transparente, la barra se correría tres píxeles al abrir una
 * pestaña contra un servidor con entorno.
 */
export function envBar(environment: Environment | null): string {
  return environment === null ? "border-l-[3px] border-l-transparent" : ENVIRONMENTS[environment].bar;
}

/** Conexión abierta en solo lectura: el servidor rechaza toda escritura. */
export const READ_ONLY_LOOK = {
  icon: "lock" as IconName,
  label: "solo lectura",
  tone: "tag-neutral",
  title: "La conexión se abre en solo lectura: el servidor rechaza cualquier escritura",
};

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
    extension: "Extensión",
    foreignDataWrapper: "Wrapper foráneo",
    foreignServer: "Servidor foráneo",
  };
  return names[kind as string] ?? String(kind);
}

/**
 * Ícono, color y nombre de lo que aparece en el informe de comparación.
 *
 * El árbol no distingue entre una enumeración, un compuesto y un dominio —los tres son un «tipo»—,
 * pero la comparación sí, porque lo que se puede hacer con cada uno es distinto. El ícono y el color
 * los sigue poniendo `lookOf`, que es el único lugar donde vive esa tabla.
 */
export function compareLook(kind: CompareObject): NodeLook {
  switch (kind) {
    case "enum":
    case "composite":
    case "domain":
    case "range":
      return lookOf("type");
    default:
      return lookOf(kind);
  }
}

export function compareLabel(kind: CompareObject): string {
  switch (kind) {
    case "enum":
      return "Enumeración";
    case "composite":
      return "Tipo compuesto";
    case "domain":
      return "Dominio";
    case "range":
      return "Rango";
    default:
      return kindLabel(kind);
  }
}
