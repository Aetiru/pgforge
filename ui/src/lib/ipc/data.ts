import { invoke } from "./core";

// ---------------------------------------------------------------------------
// Datos de una tabla
// ---------------------------------------------------------------------------

export interface TableColumn {
  name: string;
  /** Como lo escribe `format_type`: ya es válido para usar en SQL. */
  typeName: string;
  notNull: boolean;
  default: string | null;
  /** La calcula el servidor (identidad o generada): no se puede escribir. */
  generated: boolean;
  comment: string | null;
}

export interface TableKey {
  name: string;
  kind: "primary" | "unique";
  columns: string[];
}

export interface TableShape {
  oid: number;
  schema: string;
  name: string;
  columns: TableColumn[];
  key: TableKey | null;
  /** Por qué no se puede editar. `null` significa que sí se puede. */
  readOnly: string | null;
}

export type Cursor = { kind: "after"; key: string[] } | { kind: "offset"; rows: number };

export interface PageOrder {
  column: string;
  descending: boolean;
}

/** Con qué orden y con qué filtro se mira la tabla. Los resuelve el servidor, no la grilla. */
export interface PageView {
  order?: PageOrder | null;
  /** Predicado del `WHERE`, tal como lo escribió el usuario. */
  filter?: string | null;
}

export interface DataPage {
  columns: string[];
  rows: (string | null)[][];
  /** Con qué pedir la página siguiente. `null` cuando ya no hay más. */
  next: Cursor | null;
}

/** Valores de una fila, por nombre de columna. `null` es un NULL de la base. */
export type RowValues = Record<string, string | null>;

export type Change =
  | { kind: "insert"; values: RowValues }
  | { kind: "update"; key: string[]; original: RowValues; changes: RowValues }
  | { kind: "delete"; key: string[] };

export interface PreviewStatement {
  sql: string;
  params: (string | null)[];
}

export interface Applied {
  inserted: number;
  updated: number;
  deleted: number;
}

export const dataOpen = (id: string, oid: number, database?: string) =>
  invoke<TableShape>("data_open", { id, oid, database: database ?? null });

/** La misma forma, buscada por nombre: es lo que trae la sugerencia de un plan, que no tiene oid. */
export const dataShapeNamed = (id: string, schema: string, name: string, database?: string) =>
  invoke<TableShape>("data_shape_named", { id, schema, name, database: database ?? null });

export const dataPage = (
  id: string,
  shape: TableShape,
  cursor: Cursor | null,
  limit?: number,
  database?: string,
  view?: PageView,
) =>
  invoke<DataPage>("data_page", {
    id,
    shape,
    cursor,
    limit: limit ?? null,
    database: database ?? null,
    view: view ?? null,
  });

export const dataPreview = (shape: TableShape, changes: Change[]) =>
  invoke<PreviewStatement[]>("data_preview", { shape, changes });

export const dataApply = (id: string, shape: TableShape, changes: Change[], database?: string) =>
  invoke<Applied>("data_apply", { id, shape, changes, database: database ?? null });

// ---------------------------------------------------------------------------
// Exportar e importar con COPY
// ---------------------------------------------------------------------------

export type CopyFormat = "csv" | "text" | "binary";

export interface TextOptions {
  /** Primera línea con los nombres de columna. Solo CSV. */
  header: boolean;
  /** Separador de campos. Un solo carácter. */
  delimiter?: string;
  /** Carácter de comillas. Solo CSV. */
  quote?: string;
  /** Texto que representa un NULL (distinto de la cadena vacía). */
  null?: string;
}

/** De dónde salen las filas a exportar. */
export type ExportSource =
  | { kind: "table"; schema: string; table: string; columns: string[] }
  | { kind: "query"; sql: string };

export interface ExportSpec {
  source: ExportSource;
  format: CopyFormat;
  options: TextOptions;
}

export interface ImportSpec {
  schema: string;
  table: string;
  /** Vacío usa todas las columnas de la tabla, en su orden. */
  columns: string[];
  format: CopyFormat;
  options: TextOptions;
}

/** El texto del COPY que se ejecutaría. Lo que muestra la vista previa. */
export interface CopyCommand {
  sql: string;
}

export const dataExportPreview = (spec: ExportSpec) =>
  invoke<CopyCommand>("data_export_preview", { spec });

/**
 * Lanza la exportación y devuelve el identificador de su proceso.
 *
 * `target` es el rótulo con el que se la muestra —la tabla, o de qué pestaña salió la consulta—:
 * lo arma la interfaz porque es texto que se lee, no un dato del que dependa la exportación.
 */
export const dataExportRun = (
  id: string,
  spec: ExportSpec,
  path: string,
  target: string,
  database?: string,
) => invoke<string>("data_export_run", { id, spec, path, target, database: database ?? null });

export const dataImportPreview = (spec: ImportSpec) =>
  invoke<CopyCommand>("data_import_preview", { spec });

export const dataImportRun = (
  id: string,
  spec: ImportSpec,
  path: string,
  database?: string,
) => invoke<string>("data_import_run", { id, spec, path, database: database ?? null });
