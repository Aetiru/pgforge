import { Channel } from "@tauri-apps/api/core";

import { invoke, type CoreError } from "./core";

// ---------------------------------------------------------------------------
// Consultas
// ---------------------------------------------------------------------------

export interface QueryTab {
  tabId: string;
  database: string;
  autocommit: boolean;
  txStatus: TxStatus;
}

/** Estado de la transacción de una pestaña. `failed` es «abierta pero abortada». */
export type TxStatus = "idle" | "active" | "failed";

export interface QueryLimits {
  maxRows: number;
}

/** Resultado de una sentencia. Refleja `pgforge_core::sql::Outcome`. */
export type Outcome =
  | {
      kind: "rows";
      columns: string[];
      /** `null` es un NULL de la base, distinto de la cadena vacía. */
      rows: (string | null)[][];
      rowCount: number;
      truncated: boolean;
      seconds: number;
    }
  | { kind: "command"; tag: string; affected: number; seconds: number };

export type QueryEvent =
  | { type: "started"; index: number; total: number; line: number; offset: number }
  | { type: "finished"; index: number; outcome: Outcome }
  | { type: "notice"; severity: string; message: string }
  | { type: "failed"; index: number; error: CoreError; offset: number }
  | { type: "transaction"; status: TxStatus }
  | { type: "completed"; seconds: number; executed: number };

export interface ExplainOptions {
  analyze: boolean;
  buffers: boolean;
  verbose: boolean;
}

export interface PlanNode {
  nodeType: string;
  relation: string | null;
  /** El esquema de la relación; llega porque el plan se pide con VERBOSE. */
  schema: string | null;
  index: string | null;
  condition: string | null;
  /** El `Filter` suelto: en un `Index Scan`, lo que el índice no resolvió. */
  filter: string | null;
  startupCost: number;
  totalCost: number;
  planRows: number;
  actualRows: number | null;
  loops: number | null;
  totalMs: number | null;
  /** Tiempo del nodo sin el de sus hijos: el que señala al culpable. */
  selfMs: number | null;
  rowsRemoved: number | null;
  misestimated: boolean;
  sharedHitBlocks: number | null;
  sharedReadBlocks: number | null;
  sortMethod: string | null;
  sortSpaceKb: number | null;
  /** El orden no entró en `work_mem` y terminó escribiéndose en disco. */
  sortOnDisk: boolean;
  children: PlanNode[];
}

export type AdviceKind = "missingIndex" | "indexFilter" | "staleStats" | "workMem";

/** Lo que conviene mirar del plan. Nunca se aplica solo: la sentencia se copia (ver `sql::advice`). */
/** Sobre qué tabla y por qué columnas se propone el índice, ya desarmado para el diálogo. */
export interface IndexTarget {
  schema: string;
  table: string;
  columns: string[];
}

export interface Advice {
  kind: AdviceKind;
  severity: "warn" | "info";
  title: string;
  detail: string;
  sql: string | null;
  index: IndexTarget | null;
}

export interface Plan {
  root: PlanNode;
  planningMs: number | null;
  executionMs: number | null;
  analyzed: boolean;
  advice: Advice[];
  /** La respuesta del servidor tal cual, para pegarla en un visor de planes. */
  json: string;
}

export const queryOpen = (id: string, database?: string) =>
  invoke<QueryTab>("query_open", { id, database: database ?? null });

export const queryClose = (tabId: string) => invoke<void>("query_close", { tabId });

export const queryRun = (
  tabId: string,
  sql: string,
  channel: Channel<QueryEvent>,
  limits?: QueryLimits,
) => invoke<void>("query_run", { tabId, sql, channel, limits: limits ?? null });

export const queryCancel = (tabId: string) => invoke<void>("query_cancel", { tabId });

/** Nombre y tipo de cada columna del resultado. */
export interface ColumnType {
  name: string;
  typeName: string;
}

/**
 * Los tipos de las columnas de una sentencia, sin ejecutarla. Cuesta prepararla en el servidor, así
 * que se pide solo cuando el usuario quiere verlos.
 */
export const queryColumnTypes = (tabId: string, sql: string) =>
  invoke<ColumnType[]>("query_column_types", { tabId, sql });

export const queryCommit = (tabId: string) => invoke<TxStatus>("query_commit", { tabId });

export const queryRollback = (tabId: string) => invoke<TxStatus>("query_rollback", { tabId });

/** Encenderlo no confirma la transacción abierta: solo deja de abrir una nueva en cada ejecución. */
export const queryAutocommit = (tabId: string, enabled: boolean) =>
  invoke<TxStatus>("query_autocommit", { tabId, enabled });

export const queryTxStatus = (tabId: string) => invoke<TxStatus>("query_tx_status", { tabId });

export const queryExplain = (tabId: string, sql: string, options?: ExplainOptions) =>
  invoke<Plan>("query_explain", { tabId, sql, options: options ?? null });

export interface RelationColumn {
  name: string;
  typeName: string;
  comment?: string | null;
}

export interface SchemaRelation {
  /** Para el `Ctrl`+clic que revela la tabla en el árbol. */
  oid: number;
  schema: string;
  name: string;
  comment?: string | null;
  columns: RelationColumn[];
}

export interface SchemaSnapshot {
  database: string;
  schemas: string[];
  relations: SchemaRelation[];
}

export const schemaSnapshot = (id: string, database?: string) =>
  invoke<SchemaSnapshot>("schema_snapshot", { id, database: database ?? null });

export interface SqlStatement {
  text: string;
  /** Desplazamiento del primer carácter dentro del script, en caracteres. */
  offset: number;
  line: number;
}

export interface HistoryEntry {
  id: number;
  /** De dónde salió: escrito en el editor, o generado y aplicado por un diálogo. */
  source: "editor" | "dialog";
  profileId: string;
  database: string;
  sql: string;
  /** Segundos desde el epoch. */
  startedAt: number;
  seconds: number;
  rowCount: number | null;
  succeeded: boolean;
  error: string | null;
}

export const historyRecent = (id?: string, limit?: number) =>
  invoke<HistoryEntry[]>("history_recent", { id: id ?? null, limit: limit ?? null });

export const historySearch = (text: string, limit?: number) =>
  invoke<HistoryEntry[]>("history_search", { text, limit: limit ?? null });

export const historyClear = () => invoke<void>("history_clear");

/**
 * Una consulta que el usuario decidió conservar, con nombre. Espejo de `pgforge_core::sql::saved`.
 *
 * Distinta del historial: eso es lo que pasó y se vacía sin que duela; esto es lo que se guarda a
 * propósito. El servidor y la base son de dónde salió, no una atadura: se puede abrir contra otra.
 */
export interface SavedQuery {
  id: number;
  name: string;
  sql: string;
  profileId: string | null;
  database: string | null;
  /** Segundos desde el epoch. */
  createdAt: number;
  updatedAt: number;
}

export const savedList = () => invoke<SavedQuery[]>("saved_list");

/** Con `id` reescribe esa consulta; sin él guarda una nueva. Un nombre repetido devuelve conflicto. */
export const savedSave = (query: {
  id?: number | null;
  name: string;
  sql: string;
  profileId?: string | null;
  database?: string | null;
}) =>
  invoke<SavedQuery>("saved_save", {
    query: {
      id: query.id ?? null,
      name: query.name,
      sql: query.sql,
      profileId: query.profileId ?? null,
      database: query.database ?? null,
    },
  });

export const savedDelete = (savedId: number) => invoke<boolean>("saved_delete", { savedId });

export const statementAtCursor = (sql: string, cursor: number) =>
  invoke<SqlStatement | null>("statement_at_cursor", { sql, cursor });

/**
 * Formatea un script SQL. Nunca falla: `sql::format` es pura y, si no entiende algo, la guarda de
 * equivalencia del núcleo devuelve el texto tal cual llegó en vez de arriesgarse a romperlo.
 */
export const sqlFormat = (sql: string) => invoke<string>("sql_format", { sql });

export const explainWarning = (sql: string, options?: ExplainOptions) =>
  invoke<string | null>("explain_warning", { sql, options: options ?? null });

/** Guarda el texto de una pestaña de consulta como archivo. */
export const sqlWriteFile = (path: string, sql: string) =>
  invoke<void>("sql_write_file", { path, sql });

export const sqlReadFile = (path: string) => invoke<string>("sql_read_file", { path });