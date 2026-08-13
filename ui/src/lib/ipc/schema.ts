/**
 * El árbol de objetos y lo que se lee de él: nodos, búsqueda contra el servidor, DDL de lectura y
 * el grafo del diagrama.
 */

import { invoke } from "./core";

import type { RefAction } from "./ddl";

export type FolderKind =
  | "schemas"
  | "tables"
  | "views"
  | "materializedViews"
  | "foreignTables"
  | "sequences"
  | "functions"
  | "procedures"
  | "types"
  | "columns"
  | "indexes"
  | "constraints"
  | "triggers"
  | "policies"
  | "roles"
  | "extensions"
  | "fdws"
  | "fservers";

export type NodeKind =
  | "database"
  | "schema"
  | "table"
  | "partitionedTable"
  | "foreignTable"
  | "view"
  | "materializedView"
  | "sequence"
  | "function"
  | "procedure"
  | "type"
  | "column"
  | "index"
  | "constraint"
  | "trigger"
  | "policy"
  | "role"
  | "extension"
  | "foreignDataWrapper"
  | "foreignServer"
  | { folder: FolderKind };

/** Rasgo de un objeto que se muestra como etiqueta junto al nombre en el árbol. */
export type NodeTag = "login" | "group" | "superuser" | "partition" | "rowSecurity";

export interface TreeNode {
  id: string;
  label: string;
  detail?: string;
  kind: NodeKind;
  hasChildren: boolean;
  database: string;
  schema?: string;
  oid?: number;
  comment?: string;
  /** Ausente cuando no tiene ninguno. */
  tags?: NodeTag[];
}

export interface TreeOptions {
  showSystemSchemas: boolean;
}

/**
 * Una coincidencia de la búsqueda contra el servidor. Refleja `introspect::SearchHit`.
 *
 * Trae el esquema, el OID y el tipo porque con eso alcanza para abrir el camino hasta el objeto en
 * el árbol: el tipo dice en qué carpeta vive.
 */
export interface SearchHit {
  kind: NodeKind;
  database: string;
  schema: string;
  label: string;
  detail?: string;
  oid: number;
}

export interface Ddl {
  sql: string;
  source: "catalog" | "pgDump";
}

export interface GraphColumn {
  /** `attnum`. Identifica la columna dentro de la tabla, no su posición en la caja. */
  position: number;
  name: string;
  typeName: string;
  notNull: boolean;
  primaryKey: boolean;
  /** Participa de alguna clave foránea saliente. */
  foreignKey: boolean;
}

export interface GraphTable {
  oid: number;
  name: string;
  kind: NodeKind;
  comment?: string;
  columns: GraphColumn[];
}

export interface GraphEdge {
  /** Nombre de la restricción. */
  name: string;
  source: number;
  target: number;
  sourceColumns: string[];
  targetColumns: string[];
  onUpdate: RefAction;
  onDelete: RefAction;
  /** `esquema.tabla` cuando la referida está fuera del diagrama; entonces `target` no es un nodo. */
  targetLabel?: string;
}

export interface SchemaGraph {
  database: string;
  schema: string;
  tables: GraphTable[];
  edges: GraphEdge[];
}

/**
 * Hijos de un nodo del árbol.
 *
 * `requestId` es opcional y solo sirve para poder cancelar: con él, `readCancel` aborta la consulta
 * del lado del servidor en vez de dejar a la ventana esperando algo que ya no le importa a nadie.
 */
export const treeChildren = (
  id: string,
  parent: TreeNode | null,
  options: TreeOptions,
  requestId?: string,
) => invoke<TreeNode[]>("tree_children", { id, parent, options, requestId: requestId ?? null });

/** Aborta una lectura en curso. Si ya terminó no hace nada, y eso no es un error. */
export const readCancel = (requestId: string) => invoke<void>("read_cancel", { requestId });

/**
 * Busca objetos por nombre en una base, contra el catálogo del servidor. Es lo que el filtro del
 * árbol no puede hacer: ese solo alcanza lo que ya se trajo.
 */
export const treeSearch = (
  id: string,
  database: string,
  pattern: string,
  options: TreeOptions,
  limit?: number,
) => invoke<SearchHit[]>("tree_search", { id, database, pattern, options, limit: limit ?? null });

export const objectDdl = (id: string, node: TreeNode, requestId?: string) =>
  invoke<Ddl>("object_ddl", { id, node, requestId: requestId ?? null });

/** Tablas y claves foráneas de un esquema. Sin posiciones: el layout lo calcula `erd.ts`. */
export const schemaGraph = (id: string, database: string, schema: string) =>
  invoke<SchemaGraph>("schema_graph", { id, database, schema });

/** Guarda el SVG del diagrama, que arma la interfaz, en la ruta que eligió el usuario. */
export const erdExportSvg = (path: string, svg: string) =>
  invoke<void>("erd_export_svg", { path, svg });


export function folderOf(kind: NodeKind): FolderKind | null {
  return typeof kind === "object" ? kind.folder : null;
}
