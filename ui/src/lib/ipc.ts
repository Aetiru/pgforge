import { Channel, invoke } from "@tauri-apps/api/core";

/** Error del núcleo tal como cruza el IPC. Refleja `pgforge_core::error::ErrorPayload`. */
export type CoreError =
  | { kind: "canceled" }
  /** Los datos cambiaron entre que se leyeron y se quisieron escribir. */
  | { kind: "conflict"; message: string }
  | { kind: "permission"; message: string }
  | {
      kind: "database";
      code: string;
      message: string;
      detail: string | null;
      hint: string | null;
      position: number | null;
    }
  | { kind: "other"; message: string };

export type SslMode = "disable" | "prefer" | "require" | "verifyCa" | "verifyFull";

export interface SshTunnel {
  host: string;
  port: number;
  user: string;
  privateKey?: string;
}

export interface ConnectionProfile {
  id: string;
  name: string;
  group?: string;
  host: string;
  port: number;
  database: string;
  user: string;
  sslMode: SslMode;
  rootCert?: string;
  connectTimeoutSecs: number;
  statementTimeoutMs?: number;
  tunnel?: SshTunnel;
  savePassword: boolean;
}

export interface ServerCaps {
  /** `server_version_num`: mayor * 10000 + menor. */
  version: number;
  currentUser: string;
  currentDatabase: string;
  isSuperuser: boolean;
  canSignalBackends: boolean;
  canReadAllStats: boolean;
}

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
  | "triggers";

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
  | { folder: FolderKind };

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
}

export interface TreeOptions {
  showSystemSchemas: boolean;
}

export interface Ddl {
  sql: string;
  source: "catalog" | "pgDump";
}

export interface AppInfo {
  version: string;
  minPostgresMajor: number;
}

export interface Connected {
  profile: ConnectionProfile;
  caps: ServerCaps;
}

export const appInfo = () => invoke<AppInfo>("app_info");

export const listProfiles = () => invoke<ConnectionProfile[]>("list_profiles");

export const saveProfile = (profile: ConnectionProfile, password?: string) =>
  invoke<ConnectionProfile>("save_profile", { profile, password: password || null });

export const deleteProfile = (id: string) => invoke<void>("delete_profile", { id });

export const connect = (id: string, password?: string) =>
  invoke<Connected>("connect", { id, password: password || null });

export const disconnect = (id: string) => invoke<void>("disconnect", { id });

export const connectedServers = () => invoke<string[]>("connected_servers");

export const treeChildren = (id: string, parent: TreeNode | null, options: TreeOptions) =>
  invoke<TreeNode[]>("tree_children", { id, parent, options });

export const objectDdl = (id: string, node: TreeNode) =>
  invoke<Ddl>("object_ddl", { id, node });

// ---------------------------------------------------------------------------
// Monitoreo
// ---------------------------------------------------------------------------

export interface Backend {
  pid: number;
  database: string | null;
  user: string | null;
  applicationName: string;
  clientAddr: string | null;
  backendType: string;
  state: string | null;
  waitEventType: string | null;
  waitEvent: string | null;
  query: string | null;
  queryId: number | null;
  leaderPid: number | null;
  querySeconds: number | null;
  transactionSeconds: number | null;
  stateSeconds: number | null;
  blockedBy: number[];
  isMonitor: boolean;
}

export interface BlockNode {
  pid: number;
  blocking: BlockNode[];
}

export interface Metrics {
  totalConnections: number;
  activeConnections: number;
  idleInTransaction: number;
  waitingConnections: number;
  maxConnections: number;
  transactionsPerSecond: number | null;
  cacheHitRatio: number | null;
  longestTransactionSeconds: number | null;
}

export interface Snapshot {
  backends: Backend[];
  blocking: BlockNode[];
  metrics: Metrics;
}

export type MonitorEvent =
  | { type: "snapshot"; snapshot: Snapshot }
  | { type: "error"; error: CoreError };

export interface ActivityFilter {
  includeIdle: boolean;
  includeBackground: boolean;
  database: string | null;
}

export interface MonitorOptions {
  intervalMs?: number;
  paused?: boolean;
  filter?: ActivityFilter;
}

export interface Lock {
  lockType: string;
  relation: string | null;
  mode: string;
  granted: boolean;
}

export interface TableStat {
  schema: string;
  table: string;
  liveTuples: number;
  deadTuples: number;
  deadRatio: number | null;
  totalBytes: number;
  tableBytes: number;
  indexBytes: number;
  sequentialScans: number;
  indexScans: number | null;
  lastVacuumSeconds: number | null;
  lastAutovacuumSeconds: number | null;
  lastAnalyzeSeconds: number | null;
}

export interface IndexStat {
  schema: string;
  table: string;
  index: string;
  scans: number;
  bytes: number;
  isUnique: boolean;
  isPrimary: boolean;
  isValid: boolean;
}

export interface StatementStat {
  queryId: number | null;
  database: string | null;
  user: string | null;
  query: string;
  calls: number;
  totalMs: number;
  meanMs: number;
  rows: number;
}

export type Operation =
  | { kind: "vacuum"; full: boolean; freeze: boolean; analyze: boolean }
  | { kind: "analyze" }
  | { kind: "reindex"; concurrently: boolean };

export type Target =
  | { kind: "database"; name: string }
  | { kind: "table"; schema: string; name: string };

export interface MaintenancePlan {
  sql: string;
  warning: string | null;
}

export type MaintenanceEvent =
  | { type: "started"; sql: string }
  | { type: "notice"; severity: string; message: string }
  | { type: "finished"; seconds: number }
  | { type: "failed"; error: CoreError };

export const monitorStart = (
  id: string,
  channel: Channel<MonitorEvent>,
  options?: MonitorOptions,
  database?: string,
) => invoke<void>("monitor_start", { id, channel, options: options ?? null, database: database ?? null });

export const monitorStop = (id: string) => invoke<void>("monitor_stop", { id });

export const monitorConfigure = (id: string, options: MonitorOptions) =>
  invoke<void>("monitor_configure", { id, options });

export const monitorRefresh = (id: string, filter?: ActivityFilter) =>
  invoke<Snapshot>("monitor_refresh", { id, filter: filter ?? null });

export const backendLocks = (id: string, pid: number) =>
  invoke<Lock[]>("backend_locks", { id, pid });

export const cancelBackend = (id: string, pid: number) =>
  invoke<boolean>("cancel_backend", { id, pid });

export const terminateBackend = (id: string, pid: number) =>
  invoke<boolean>("terminate_backend", { id, pid });

export const tableStats = (id: string, limit?: number) =>
  invoke<TableStat[]>("table_stats", { id, limit: limit ?? null });

export const indexStats = (id: string, limit?: number) =>
  invoke<IndexStat[]>("index_stats", { id, limit: limit ?? null });

export const hasStatementStats = (id: string) => invoke<boolean>("has_statement_stats", { id });

export const statementStats = (id: string, limit?: number) =>
  invoke<StatementStat[]>("statement_stats", { id, limit: limit ?? null });

export const maintenancePlan = (id: string, operation: Operation, target: Target) =>
  invoke<MaintenancePlan>("maintenance_plan", { id, operation, target });

export const maintenanceRun = (
  id: string,
  operation: Operation,
  target: Target,
  channel: Channel<MaintenanceEvent>,
  database?: string,
) =>
  invoke<string>("maintenance_run", {
    id,
    operation,
    target,
    channel,
    database: database ?? null,
  });

export const maintenanceCancel = (taskId: string) =>
  invoke<void>("maintenance_cancel", { taskId });

// ---------------------------------------------------------------------------
// Consultas
// ---------------------------------------------------------------------------

export interface QueryTab {
  tabId: string;
  database: string;
}

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
  | { type: "started"; index: number; total: number; line: number }
  | { type: "finished"; index: number; outcome: Outcome }
  | { type: "notice"; severity: string; message: string }
  | { type: "failed"; index: number; error: CoreError; offset: number }
  | { type: "completed"; seconds: number; executed: number };

export interface ExplainOptions {
  analyze: boolean;
  buffers: boolean;
  verbose: boolean;
}

export interface PlanNode {
  nodeType: string;
  relation: string | null;
  index: string | null;
  condition: string | null;
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
  children: PlanNode[];
}

export interface Plan {
  root: PlanNode;
  planningMs: number | null;
  executionMs: number | null;
  analyzed: boolean;
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

export const queryExplain = (tabId: string, sql: string, options?: ExplainOptions) =>
  invoke<Plan>("query_explain", { tabId, sql, options: options ?? null });

export interface SchemaRelation {
  schema: string;
  name: string;
  columns: string[];
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

export const statementAtCursor = (sql: string, cursor: number) =>
  invoke<SqlStatement | null>("statement_at_cursor", { sql, cursor });

export const explainWarning = (sql: string, options?: ExplainOptions) =>
  invoke<string | null>("explain_warning", { sql, options: options ?? null });

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

export const dataPage = (
  id: string,
  shape: TableShape,
  cursor: Cursor | null,
  limit?: number,
  database?: string,
) =>
  invoke<DataPage>("data_page", {
    id,
    shape,
    cursor,
    limit: limit ?? null,
    database: database ?? null,
  });

export const dataPreview = (shape: TableShape, changes: Change[]) =>
  invoke<PreviewStatement[]>("data_preview", { shape, changes });

export const dataApply = (id: string, shape: TableShape, changes: Change[], database?: string) =>
  invoke<Applied>("data_apply", { id, shape, changes, database: database ?? null });

// ---------------------------------------------------------------------------
// Estructura de tablas
// ---------------------------------------------------------------------------

export type Identity = "always" | "byDefault";

export interface ColumnDef {
  name: string;
  /** Texto crudo: lo valida el servidor al ejecutar, acá no se interpreta. */
  typeName: string;
  notNull: boolean;
  /** Expresión SQL cruda (p. ej. `now()`), no un literal a escapar. */
  default: string | null;
  identity: Identity | null;
}

export type TableChange =
  | { kind: "createTable"; schema: string; name: string; columns: ColumnDef[] }
  | { kind: "dropTable"; schema: string; name: string; cascade: boolean }
  | { kind: "renameTable"; schema: string; name: string; newName: string }
  | { kind: "addColumn"; schema: string; table: string; column: ColumnDef }
  | { kind: "dropColumn"; schema: string; table: string; column: string; cascade: boolean }
  | { kind: "renameColumn"; schema: string; table: string; column: string; newName: string }
  | {
      kind: "alterColumnType";
      schema: string;
      table: string;
      column: string;
      typeName: string;
      /** Solo hace falta cuando el cambio de tipo no es implícito. */
      using: string | null;
    }
  | { kind: "setColumnNotNull"; schema: string; table: string; column: string; notNull: boolean }
  | { kind: "setColumnDefault"; schema: string; table: string; column: string; default: string | null };

/** El DDL no admite parámetros: a diferencia de `PreviewStatement`, el texto ya está completo. */
export interface DdlStatement {
  sql: string;
}

export const ddlPreview = (changes: TableChange[]) =>
  invoke<DdlStatement[]>("ddl_preview", { changes });

export const ddlApply = (id: string, changes: TableChange[], database?: string) =>
  invoke<void>("ddl_apply", { id, changes, database: database ?? null });

export { Channel };

/** `160004` se muestra como `16.4`. */
export function formatVersion(versionNum: number): string {
  return `${Math.floor(versionNum / 10000)}.${versionNum % 10000}`;
}

export function folderOf(kind: NodeKind): FolderKind | null {
  return typeof kind === "object" ? kind.folder : null;
}

/** `true` si el error viene de una cancelación pedida por el usuario, que no es una falla. */
export function isCanceled(error: unknown): boolean {
  return typeof error === "object" && error !== null && (error as CoreError).kind === "canceled";
}

/** Texto legible para cualquier error del núcleo. */
export function describeError(error: unknown): string {
  const e = error as CoreError;
  switch (e?.kind) {
    case "canceled":
      return "Operación cancelada.";
    case "conflict":
      return e.message;
    case "permission":
      return `Permiso insuficiente: ${e.message}`;
    case "database":
      return e.hint ? `${e.message} (${e.hint})` : e.message;
    case "other":
      return e.message;
    default:
      return String(error);
  }
}
