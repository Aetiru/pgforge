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
  /** La clave del host SSH no está en `known_hosts`, o cambió. La interfaz muestra la huella. */
  | { kind: "sshHostKey"; host: string; fingerprint: string; changed: boolean }
  | { kind: "other"; message: string };

export type SslMode = "disable" | "prefer" | "require" | "verifyCa" | "verifyFull";

/** Para qué se usa el servidor. No cambia cómo se conecta: cambia cuánto se avisa antes de tocarlo. */
export type Environment = "dev" | "test" | "prod";

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
  environment?: Environment;
  /** Abre toda conexión al servidor con `default_transaction_read_only`. */
  readOnly: boolean;
  /** Valor inicial del autocommit de cada pestaña de consulta. */
  autocommit: boolean;
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

export const saveProfile = (profile: ConnectionProfile, password?: string, sshPassword?: string) =>
  invoke<ConnectionProfile>("save_profile", {
    profile,
    password: password || null,
    sshPassword: sshPassword || null,
  });

export const deleteProfile = (id: string) => invoke<void>("delete_profile", { id });

/** Las carpetas en las que están repartidos los servidores guardados. */
export const listGroups = () => invoke<string[]>("list_groups");

/** Renombra una carpeta, o la deshace si no se pasa nombre nuevo. Devuelve cuántos se movieron. */
export const renameGroup = (from: string, to?: string) =>
  invoke<number>("rename_group", { from, to: to ?? null });

export const connect = (
  id: string,
  password?: string,
  sshPassword?: string,
  trustHostKey?: boolean,
) =>
  invoke<Connected>("connect", {
    id,
    password: password || null,
    sshPassword: sshPassword || null,
    trustHostKey: trustHostKey ?? null,
  });

/**
 * Prueba el túnel SSH del perfil sin conectar a la base. Devuelve el error `sshHostKey` si la clave
 * del bastión no está verificada, igual que `connect`, para reusar el mismo flujo de confirmación.
 */
export const sshTest = (profile: ConnectionProfile, sshPassword?: string, trustHostKey?: boolean) =>
  invoke<void>("ssh_test", {
    profile,
    sshPassword: sshPassword || null,
    trustHostKey: trustHostKey ?? null,
  });

export const disconnect = (id: string) => invoke<void>("disconnect", { id });

export const connectedServers = () => invoke<string[]>("connected_servers");

export const treeChildren = (id: string, parent: TreeNode | null, options: TreeOptions) =>
  invoke<TreeNode[]>("tree_children", { id, parent, options });

export const objectDdl = (id: string, node: TreeNode) =>
  invoke<Ddl>("object_ddl", { id, node });

/** Tablas y claves foráneas de un esquema. Sin posiciones: el layout lo calcula `erd.ts`. */
export const schemaGraph = (id: string, database: string, schema: string) =>
  invoke<SchemaGraph>("schema_graph", { id, database, schema });

/** Guarda el SVG del diagrama, que arma la interfaz, en la ruta que eligió el usuario. */
export const erdExportSvg = (path: string, svg: string) =>
  invoke<void>("erd_export_svg", { path, svg });

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

export interface TableBloat {
  schema: string;
  table: string;
  totalBytes: number;
  /** Espacio libre estimado dentro de la tabla, en bytes. */
  freeBytes: number;
  /** Fracción del espacio que está libre (0 a 1). */
  freeRatio: number;
  /** Fracción ocupada por tuplas muertas que el vacuum todavía no limpió (0 a 1). */
  deadRatio: number;
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
  | { kind: "table"; schema: string; name: string }
  | { kind: "index"; schema: string; name: string };

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

export const hasBloatStats = (id: string) => invoke<boolean>("has_bloat_stats", { id });

export const tableBloat = (id: string, limit?: number) =>
  invoke<TableBloat[]>("table_bloat", { id, limit: limit ?? null });

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
// Backups
// ---------------------------------------------------------------------------

export type BackupFormat = "plain" | "custom" | "directory" | "tar";

export interface BackupOptions {
  database: string;
  format: BackupFormat;
  /** Archivo de salida, o directorio en el formato correspondiente. */
  path: string;
  /** Vacío quiere decir todos. */
  schemas: string[];
  excludeSchemas: string[];
  /** Como `esquema.tabla`, o solo el nombre. */
  tables: string[];
  schemaOnly: boolean;
  dataOnly: boolean;
  noOwner: boolean;
  noPrivileges: boolean;
  compression: number | null;
  /** Solo el formato directorio admite más de uno. */
  jobs: number | null;
}

export interface BackupPlan {
  /** El binario y sus argumentos. La contraseña no está acá: viaja por el entorno. */
  command: string[];
  warning: string | null;
}

export type BackupEvent =
  | { type: "started"; command: string[] }
  | { type: "progress"; message: string }
  | { type: "finished"; path: string; bytes: number; seconds: number }
  | { type: "failed"; error: CoreError };

export const backupPlan = (id: string, options: BackupOptions) =>
  invoke<BackupPlan>("backup_plan", { id, options });

export const backupRun = (id: string, options: BackupOptions, channel: Channel<BackupEvent>) =>
  invoke<string>("backup_run", { id, options, channel });

export const backupCancel = (taskId: string) => invoke<void>("backup_cancel", { taskId });

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

export interface RestoreOptions {
  /** El archivo del backup, o el directorio en el formato correspondiente. */
  source: string;
  /** El formato con que se hizo el backup. `plain` no se restaura con pg_restore. */
  format: BackupFormat;
  /** Con `create`, la base de mantenimiento; sin él, la base destino. */
  database: string;
  /** Vacío quiere decir todo lo que haya en el backup. */
  schemas: string[];
  /** Como `esquema.tabla`, o solo el nombre. */
  tables: string[];
  schemaOnly: boolean;
  dataOnly: boolean;
  /** Elimina cada objeto antes de recrearlo. */
  clean: boolean;
  /** Que el borrado de `clean` no falle si el objeto no existe. Solo con `clean`. */
  ifExists: boolean;
  /** Crea la base destino en vez de cargar sobre una existente. */
  create: boolean;
  noOwner: boolean;
  noPrivileges: boolean;
  /** Todo o nada. Incompatible con el paralelismo. */
  singleTransaction: boolean;
  /** Solo custom y directorio; nunca con `singleTransaction`. */
  jobs: number | null;
}

export interface RestorePlan {
  /** El binario y sus argumentos. La contraseña no está acá: viaja por el entorno. */
  command: string[];
  warning: string | null;
}

export type RestoreEvent =
  | { type: "started"; command: string[] }
  | { type: "progress"; message: string }
  | { type: "finished"; database: string; seconds: number; ignoredErrors: number }
  | { type: "failed"; error: CoreError };

export const restorePlan = (id: string, options: RestoreOptions) =>
  invoke<RestorePlan>("restore_plan", { id, options });

export const restoreRun = (id: string, options: RestoreOptions, channel: Channel<RestoreEvent>) =>
  invoke<string>("restore_run", { id, options, channel });

export const restoreCancel = (taskId: string) => invoke<void>("restore_cancel", { taskId });

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
  | { type: "started"; index: number; total: number; line: number }
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

export type ExportEvent =
  | { type: "started"; command: string }
  | { type: "progress"; bytes: number }
  | { type: "finished"; path: string; bytes: number; seconds: number }
  | { type: "failed"; error: CoreError };

export type ImportEvent =
  | { type: "started"; command: string }
  | { type: "progress"; bytes: number }
  | { type: "finished"; bytes: number; rows: number; seconds: number }
  | { type: "failed"; error: CoreError };

export const dataExportPreview = (spec: ExportSpec) =>
  invoke<CopyCommand>("data_export_preview", { spec });

export const dataExportRun = (
  id: string,
  spec: ExportSpec,
  path: string,
  channel: Channel<ExportEvent>,
  database?: string,
) => invoke<string>("data_export_run", { id, spec, path, channel, database: database ?? null });

export const dataImportPreview = (spec: ImportSpec) =>
  invoke<CopyCommand>("data_import_preview", { spec });

export const dataImportRun = (
  id: string,
  spec: ImportSpec,
  path: string,
  channel: Channel<ImportEvent>,
  database?: string,
) => invoke<string>("data_import_run", { id, spec, path, channel, database: database ?? null });

/** Corta una exportación o importación en curso. */
export const dataCopyCancel = (taskId: string) => invoke<void>("data_copy_cancel", { taskId });

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

/** Qué hacer del otro lado de una foreign key cuando la fila referenciada cambia o desaparece. */
export type RefAction = "cascade" | "setNull" | "setDefault" | "restrict" | "noAction";

/** Postgres no tiene un "ALTER CONSTRAINT": cambiarla es borrarla y agregar una nueva. */
export type ConstraintDef =
  | { kind: "primaryKey"; columns: string[] }
  | { kind: "unique"; columns: string[] }
  | {
      kind: "foreignKey";
      columns: string[];
      refSchema: string;
      refTable: string;
      refColumns: string[];
      onDelete: RefAction | null;
      onUpdate: RefAction | null;
    }
  /** Expresión SQL cruda, misma frontera de confianza que el `default` de una columna. */
  | { kind: "check"; expression: string };

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
  | { kind: "setColumnDefault"; schema: string; table: string; column: string; default: string | null }
  | { kind: "addConstraint"; schema: string; table: string; name: string; definition: ConstraintDef }
  | { kind: "dropConstraint"; schema: string; table: string; name: string; cascade: boolean };

/** El DDL no admite parámetros: a diferencia de `PreviewStatement`, el texto ya está completo. */
export interface DdlStatement {
  sql: string;
}

export const ddlPreview = (changes: TableChange[]) =>
  invoke<DdlStatement[]>("ddl_preview", { changes });

export const ddlApply = (id: string, changes: TableChange[], database?: string) =>
  invoke<void>("ddl_apply", { id, changes, database: database ?? null });

/** Una constraint tal como ya existe. Solo se puede borrar: no hay "editarla" (ver `ConstraintDef`). */
export interface ConstraintInfo {
  oid: number;
  name: string;
  /** Etiqueta para mostrar ("primaria", "foránea", ...). */
  kind: string;
  definition: string;
}

export const tableConstraints = (id: string, oid: number, database?: string) =>
  invoke<ConstraintInfo[]>("table_constraints", { id, oid, database: database ?? null });

export interface IndexDef {
  schema: string;
  table: string;
  /** Si se deja vacío, Postgres lo nombra solo. */
  name: string | null;
  unique: boolean;
  /** `btree` si se deja vacío. */
  method: string | null;
  columns: string[];
  /** Predicado crudo de un índice parcial. */
  whereClause: string | null;
  /** No bloquea la tabla mientras se construye; no se puede combinar con un lote transaccional. */
  concurrently: boolean;
}

export interface IndexInfo {
  oid: number;
  name: string;
  definition: string;
  primary: boolean;
  unique: boolean;
  /** `false` si quedó de un CREATE INDEX CONCURRENTLY que falló. */
  valid: boolean;
  method: string;
}

export const indexPreview = (def: IndexDef) => invoke<DdlStatement>("index_preview", { def });

export const indexCreate = (id: string, def: IndexDef, database?: string) =>
  invoke<void>("index_create", { id, def, database: database ?? null });

export const indexDrop = (
  id: string,
  schema: string,
  name: string,
  cascade: boolean,
  concurrently: boolean,
  database?: string,
) => invoke<void>("index_drop", { id, schema, name, cascade, concurrently, database: database ?? null });

export const tableIndexes = (id: string, oid: number, database?: string) =>
  invoke<IndexInfo[]>("table_indexes", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Vistas
// ---------------------------------------------------------------------------

export type ViewChange =
  | {
      kind: "createView";
      schema: string;
      name: string;
      columns: string[];
      query: string;
      /** `CREATE OR REPLACE VIEW` en vez de `CREATE VIEW`. */
      replace: boolean;
    }
  | { kind: "dropView"; schema: string; name: string; cascade: boolean }
  | {
      kind: "createMaterializedView";
      schema: string;
      name: string;
      columns: string[];
      query: string;
      /** `false` deja la vista vacía hasta el próximo refresh (`WITH NO DATA`). */
      withData: boolean;
    }
  | { kind: "dropMaterializedView"; schema: string; name: string; cascade: boolean }
  | {
      kind: "refreshMaterializedView";
      schema: string;
      name: string;
      /** No bloquea a los lectores mientras se refresca; necesita un índice único. */
      concurrently: boolean;
    };

export const viewPreview = (changes: ViewChange[]) =>
  invoke<DdlStatement[]>("view_preview", { changes });

export const viewApply = (id: string, changes: ViewChange[], database?: string) =>
  invoke<void>("view_apply", { id, changes, database: database ?? null });

/** El cuerpo del SELECT, sin el `CREATE VIEW ... AS` alrededor: para precargar el editor. */
export const viewQuery = (id: string, oid: number, database?: string) =>
  invoke<string>("view_query", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Secuencias
// ---------------------------------------------------------------------------

/** La columna que posee la secuencia: al borrarse la columna se borra la secuencia con ella. */
export interface OwnedBy {
  schema: string;
  table: string;
  column: string;
}

/**
 * A qué columna se ata la secuencia, o `none` para desatarla.
 *
 * Es una unión etiquetada y no un `OwnedBy | null` porque del lado de Rust un `null` sería
 * indistinguible de «no lo toques».
 */
export type SequenceOwner =
  | { kind: "none" }
  | { kind: "column"; schema: string; table: string; column: string };

/**
 * Los parámetros de una secuencia. `null` en cualquiera es «no lo toques», no «poné el valor por
 * omisión»: en un `ALTER` la diferencia es todo.
 */
export interface SequenceOptions {
  /** `smallint`, `integer` o `bigint`. Va crudo. */
  dataType?: string | null;
  increment?: number | null;
  minValue?: number | null;
  maxValue?: number | null;
  start?: number | null;
  cache?: number | null;
  cycle?: boolean | null;
  /** `null` no la toca. */
  ownedBy?: SequenceOwner | null;
}

export type SequenceChange =
  | {
      kind: "createSequence";
      schema: string;
      name: string;
      ifNotExists: boolean;
      options: SequenceOptions;
    }
  | { kind: "alterSequence"; schema: string; name: string; options: SequenceOptions }
  /** Mueve la secuencia ahora. `START WITH` dice a dónde vuelve; esto la mueve de verdad. */
  | { kind: "restartSequence"; schema: string; name: string; value: number | null }
  | { kind: "renameSequence"; schema: string; name: string; newName: string }
  | { kind: "setSequenceSchema"; schema: string; name: string; newSchema: string }
  | { kind: "setSequenceOwner"; schema: string; name: string; owner: string }
  | { kind: "dropSequence"; schema: string; name: string; cascade: boolean };

export interface SequenceInfo {
  schema: string;
  name: string;
  owner: string;
  dataType: string;
  start: number;
  increment: number;
  minValue: number;
  maxValue: number;
  cache: number;
  cycle: boolean;
  /** `null` si todavía no se usó, y también si el rol no puede leerla: el servidor no distingue. */
  lastValue: number | null;
  ownedBy: OwnedBy | null;
  comment: string | null;
}

export const sequencePreview = (changes: SequenceChange[]) =>
  invoke<DdlStatement[]>("sequence_preview", { changes });

export const sequenceApply = (id: string, changes: SequenceChange[], database?: string) =>
  invoke<void>("sequence_apply", { id, changes, database: database ?? null });

export const sequenceInfo = (id: string, oid: number, database?: string) =>
  invoke<SequenceInfo>("sequence_info", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Tipos y dominios
// ---------------------------------------------------------------------------

/** Un campo de un tipo compuesto. El tipo va crudo, como el de una columna. */
export interface TypeField {
  name: string;
  dataType: string;
  collation?: string | null;
}

/** Dónde entra un valor nuevo de una enumeración. Sin posición va al final. */
export type EnumPosition = { kind: "before"; value: string } | { kind: "after"; value: string };

export type TypeChange =
  | { kind: "createEnum"; schema: string; name: string; labels: string[] }
  /** No hay `DROP VALUE`: sacar un valor exigiría recrear el tipo y todo lo que lo usa. */
  | {
      kind: "addEnumValue";
      schema: string;
      name: string;
      value: string;
      position: EnumPosition | null;
      ifNotExists: boolean;
    }
  | { kind: "renameEnumValue"; schema: string; name: string; from: string; to: string }
  | { kind: "createComposite"; schema: string; name: string; fields: TypeField[] }
  | { kind: "addCompositeField"; schema: string; name: string; field: TypeField }
  | { kind: "dropCompositeField"; schema: string; name: string; field: string; cascade: boolean }
  | {
      kind: "alterCompositeFieldType";
      schema: string;
      name: string;
      field: string;
      dataType: string;
      collation: string | null;
      cascade: boolean;
    }
  | { kind: "renameType"; schema: string; name: string; newName: string }
  | { kind: "setTypeSchema"; schema: string; name: string; newSchema: string }
  | { kind: "setTypeOwner"; schema: string; name: string; owner: string }
  | { kind: "dropType"; schema: string; name: string; cascade: boolean };

export type TypeKind = "enum" | "composite" | "domain" | "other";

export interface TypeInfo {
  schema: string;
  name: string;
  owner: string;
  kind: TypeKind;
  labels: string[];
  fields: TypeField[];
  comment: string | null;
}

export const typePreview = (changes: TypeChange[]) =>
  invoke<DdlStatement[]>("type_preview", { changes });

export const typeApply = (id: string, changes: TypeChange[], database?: string) =>
  invoke<void>("type_apply", { id, changes, database: database ?? null });

export const typeInfo = (id: string, oid: number, database?: string) =>
  invoke<TypeInfo>("type_info", { id, oid, database: database ?? null });

/** Una restricción `CHECK` de un dominio. `VALUE` es el valor que se está validando. */
export interface DomainConstraint {
  /** Vacío deja que el servidor lo nombre. */
  name: string | null;
  /** La expresión, cruda. */
  check: string;
  /** `NOT VALID`: no revisa lo que ya está guardado. */
  notValid: boolean;
}

export type DomainChange =
  | {
      kind: "createDomain";
      schema: string;
      name: string;
      dataType: string;
      collation: string | null;
      default: string | null;
      notNull: boolean;
      constraints: DomainConstraint[];
    }
  | { kind: "setDomainDefault"; schema: string; name: string; default: string | null }
  | { kind: "setDomainNotNull"; schema: string; name: string; notNull: boolean }
  | { kind: "addDomainConstraint"; schema: string; name: string; constraint: DomainConstraint }
  | { kind: "validateDomainConstraint"; schema: string; name: string; constraint: string }
  | {
      kind: "dropDomainConstraint";
      schema: string;
      name: string;
      constraint: string;
      ifExists: boolean;
      cascade: boolean;
    }
  | { kind: "renameDomain"; schema: string; name: string; newName: string }
  | { kind: "setDomainSchema"; schema: string; name: string; newSchema: string }
  | { kind: "setDomainOwner"; schema: string; name: string; owner: string }
  | { kind: "dropDomain"; schema: string; name: string; cascade: boolean };

export interface DomainInfo {
  schema: string;
  name: string;
  owner: string;
  dataType: string;
  collation: string | null;
  default: string | null;
  notNull: boolean;
  constraints: DomainConstraint[];
  comment: string | null;
}

export const domainPreview = (changes: DomainChange[]) =>
  invoke<DdlStatement[]>("domain_preview", { changes });

export const domainApply = (id: string, changes: DomainChange[], database?: string) =>
  invoke<void>("domain_apply", { id, changes, database: database ?? null });

export const domainInfo = (id: string, oid: number, database?: string) =>
  invoke<DomainInfo>("domain_info", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Esquemas y bases
// ---------------------------------------------------------------------------

export type SchemaChange =
  | { kind: "createSchema"; name: string; authorization: string | null; ifNotExists: boolean }
  | { kind: "renameSchema"; name: string; newName: string }
  | { kind: "setSchemaOwner"; name: string; owner: string }
  /** Sin `CASCADE` falla si tiene algo adentro, a propósito. */
  | { kind: "dropSchema"; name: string; ifExists: boolean; cascade: boolean };

export const schemaPreview = (changes: SchemaChange[]) =>
  invoke<DdlStatement[]>("schema_preview", { changes });

export const schemaApply = (id: string, changes: SchemaChange[], database?: string) =>
  invoke<void>("schema_apply", { id, changes, database: database ?? null });

/** Lo que se puede pedir al crear una base. Cada campo vacío lo decide el servidor. */
export interface DatabaseOptions {
  owner?: string | null;
  template?: string | null;
  encoding?: string | null;
  lcCollate?: string | null;
  lcCtype?: string | null;
  tablespace?: string | null;
  /** `-1` es sin límite. */
  connectionLimit?: number | null;
  isTemplate?: boolean | null;
}

export type DatabaseChange =
  | { kind: "createDatabase"; name: string; options: DatabaseOptions }
  | { kind: "renameDatabase"; name: string; newName: string }
  | { kind: "setDatabaseOwner"; name: string; owner: string }
  | { kind: "setDatabaseConnectionLimit"; name: string; limit: number }
  | { kind: "setDatabaseAllowConnections"; name: string; allow: boolean }
  /** `force` echa a las sesiones conectadas en vez de fallar. */
  | { kind: "dropDatabase"; name: string; ifExists: boolean; force: boolean };

export interface DatabaseInfo {
  name: string;
  owner: string;
  encoding: string;
  collate: string;
  ctype: string;
  tablespace: string;
  connectionLimit: number;
  allowConnections: boolean;
  isTemplate: boolean;
  /** Bytes. `null` si el rol no puede leerlo. */
  size: number | null;
  comment: string | null;
}

export const databasePreview = (changes: DatabaseChange[]) =>
  invoke<DdlStatement[]>("database_preview", { changes });

/**
 * Aplica los cambios **sin** transacción: `CREATE DATABASE` y `DROP DATABASE` no la admiten. Por eso
 * conviene mandar un cambio por vez: una lista a medias deja hecho lo anterior.
 */
export const databaseApply = (id: string, changes: DatabaseChange[]) =>
  invoke<void>("database_apply", { id, changes });

export const databaseInfo = (id: string, name: string) =>
  invoke<DatabaseInfo>("database_info", { id, name });

// ---------------------------------------------------------------------------
// Particiones
// ---------------------------------------------------------------------------

/** El límite de una partición. Los valores van crudos: admiten `MINVALUE`, `MAXVALUE` y funciones. */
export type PartitionBound =
  | { kind: "range"; from: string[]; to: string[] }
  | { kind: "list"; values: string[] }
  | { kind: "hash"; modulus: number; remainder: number }
  /** Se lleva todo lo que no entra en ninguna otra. */
  | { kind: "default" };

export type PartitionChange =
  | {
      kind: "createPartition";
      parentSchema: string;
      parent: string;
      schema: string;
      name: string;
      bound: PartitionBound;
      /** Cuando la partición es a su vez particionada: `RANGE (dia)`, crudo. */
      partitionBy: string | null;
    }
  /** El servidor revisa que ninguna fila de la tabla se salga del límite. */
  | {
      kind: "attachPartition";
      parentSchema: string;
      parent: string;
      schema: string;
      name: string;
      bound: PartitionBound;
    }
  | {
      kind: "detachPartition";
      parentSchema: string;
      parent: string;
      schema: string;
      name: string;
      /** Sin bloquear a los lectores. Pide PostgreSQL 14 o más. */
      concurrently: boolean;
      /** Termina un `DETACH … CONCURRENTLY` que quedó a medias. */
      finalize: boolean;
    }
  | { kind: "dropPartition"; schema: string; name: string; cascade: boolean };

export interface PartitionInfo {
  schema: string;
  name: string;
  /** El límite tal como lo escribe el servidor. */
  bound: string;
  partitioned: boolean;
}

export interface PartitioningInfo {
  /** La estrategia tal como la escribe el servidor: `RANGE (creado)`, `LIST (region)`, … */
  strategy: string;
  partitions: PartitionInfo[];
}

/** Pide el perfil, a diferencia de las demás vistas previas: `CONCURRENTLY` depende de la versión. */
export const partitionPreview = (id: string, changes: PartitionChange[]) =>
  invoke<DdlStatement[]>("partition_preview", { id, changes });

export const partitionApply = (id: string, changes: PartitionChange[], database?: string) =>
  invoke<void>("partition_apply", { id, changes, database: database ?? null });

export const tablePartitions = (id: string, oid: number, database?: string) =>
  invoke<PartitioningInfo>("table_partitions", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Comentarios
// ---------------------------------------------------------------------------

/** Sobre qué objeto se comenta. La lista es la de los nodos que muestra el árbol. */
export type CommentTarget =
  | { kind: "table"; schema: string; name: string }
  | { kind: "column"; schema: string; table: string; column: string }
  | { kind: "view"; schema: string; name: string }
  | { kind: "materializedView"; schema: string; name: string }
  | { kind: "foreignTable"; schema: string; name: string }
  | { kind: "sequence"; schema: string; name: string }
  | { kind: "index"; schema: string; name: string }
  | { kind: "type"; schema: string; name: string }
  | { kind: "domain"; schema: string; name: string }
  | { kind: "schema"; name: string }
  | { kind: "database"; name: string }
  | { kind: "role"; name: string }
  | { kind: "extension"; name: string }
  /** La firma hace falta para distinguir entre sobrecargas. */
  | { kind: "function"; schema: string; name: string; arguments: string }
  | { kind: "procedure"; schema: string; name: string; arguments: string }
  | { kind: "trigger"; schema: string; table: string; name: string }
  | { kind: "constraint"; schema: string; table: string; name: string }
  | { kind: "policy"; schema: string; table: string; name: string };

export interface CommentChange {
  target: CommentTarget;
  /** `null` —o en blanco— borra el comentario: vacío no es lo mismo que ninguno. */
  comment: string | null;
}

export const commentPreview = (changes: CommentChange[]) =>
  invoke<DdlStatement[]>("comment_preview", { changes });

export const commentApply = (id: string, changes: CommentChange[], database?: string) =>
  invoke<void>("comment_apply", { id, changes, database: database ?? null });

// ---------------------------------------------------------------------------
// Funciones y procedimientos
// ---------------------------------------------------------------------------

/** Ejecuta la sentencia `CREATE [OR REPLACE] FUNCTION`/`PROCEDURE` tal cual la escribió el usuario. */
export const functionApply = (id: string, sql: string, database?: string) =>
  invoke<void>("function_apply", { id, sql, database: database ?? null });

export const functionDrop = (
  id: string,
  schema: string,
  name: string,
  args: string,
  procedure: boolean,
  cascade: boolean,
  database?: string,
) =>
  invoke<void>("function_drop", {
    id,
    schema,
    name,
    args,
    procedure,
    cascade,
    database: database ?? null,
  });

/** La lista de tipos de argumento, para armar el `DROP FUNCTION`/`DROP PROCEDURE`. */
export const functionArgs = (id: string, oid: number, database?: string) =>
  invoke<string>("function_args", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Triggers
// ---------------------------------------------------------------------------

export type Timing = "before" | "after" | "insteadOf";
/** No se llama `Event`: choca con el tipo `Event` del DOM. */
export type TriggerEvent = "insert" | "update" | "delete" | "truncate";
export type TriggerLevel = "row" | "statement";

export interface TriggerDef {
  timing: Timing;
  events: TriggerEvent[];
  level: TriggerLevel;
  when: string | null;
  functionSchema: string;
  functionName: string;
}

export type TriggerChange =
  | { kind: "createTrigger"; schema: string; table: string; name: string; definition: TriggerDef }
  | { kind: "dropTrigger"; schema: string; table: string; name: string; cascade: boolean };

export interface TriggerInfo {
  oid: number;
  name: string;
  timing: Timing;
  events: TriggerEvent[];
  level: TriggerLevel;
  when: string | null;
  functionSchema: string;
  functionName: string;
}

export const triggerPreview = (changes: TriggerChange[]) =>
  invoke<DdlStatement[]>("trigger_preview", { changes });

export const triggerApply = (id: string, changes: TriggerChange[], database?: string) =>
  invoke<void>("trigger_apply", { id, changes, database: database ?? null });

export const tableTriggers = (id: string, oid: number, database?: string) =>
  invoke<TriggerInfo[]>("table_triggers", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

/** `undefined`/ausente en un campo significa "no tocar": para crear se manda todo, para editar solo lo que cambió. */
export interface RoleAttributes {
  superuser?: boolean;
  createdb?: boolean;
  createrole?: boolean;
  inherit?: boolean;
  login?: boolean;
  replication?: boolean;
  bypassRls?: boolean;
  connectionLimit?: number;
  /** `undefined` en edición: Postgres nunca devuelve la contraseña, así que no hay con qué precargarla. */
  password?: string;
  validUntil?: string;
}

export type RoleChange =
  | { kind: "createRole"; name: string; attributes: RoleAttributes; memberOf: string[] }
  | { kind: "alterRole"; name: string; attributes: RoleAttributes }
  | { kind: "renameRole"; name: string; newName: string }
  | { kind: "dropRole"; name: string }
  /** Solo alcanza a la base conectada: hay que repetirlo en cada base donde el rol tenga algo. */
  | { kind: "reassignOwned"; from: string; to: string }
  | { kind: "dropOwned"; role: string; cascade: boolean }
  | { kind: "grantMembership"; role: string; member: string; adminOption: boolean }
  | { kind: "revokeMembership"; role: string; member: string };

export interface RoleInfo {
  oid: number;
  name: string;
  superuser: boolean;
  createdb: boolean;
  createrole: boolean;
  inherit: boolean;
  login: boolean;
  replication: boolean;
  bypassRls: boolean;
  connectionLimit: number;
  validUntil: string | null;
}

export const rolePreview = (changes: RoleChange[]) =>
  invoke<DdlStatement[]>("role_preview", { changes });

export const roleApply = (id: string, changes: RoleChange[], database?: string) =>
  invoke<void>("role_apply", { id, changes, database: database ?? null });

export const roleInfo = (id: string, oid: number, database?: string) =>
  invoke<RoleInfo>("role_info", { id, oid, database: database ?? null });

export const roleMemberships = (id: string, name: string, database?: string) =>
  invoke<string[]>("role_memberships", { id, name, database: database ?? null });

// ---------------------------------------------------------------------------
// Extensiones
// ---------------------------------------------------------------------------

export type ExtensionChange =
  | {
      kind: "create";
      name: string;
      schema: string | null;
      version: string | null;
      cascade: boolean;
    }
  | { kind: "update"; name: string; version: string | null }
  | { kind: "setSchema"; name: string; schema: string }
  | { kind: "drop"; name: string; cascade: boolean };

export interface ExtensionInfo {
  name: string;
  version: string;
  schema: string;
  comment: string | null;
  /** Solo si es relocatable tiene sentido cambiarla de esquema. */
  relocatable: boolean;
  /** Puede ser más nueva que `version`: entonces hay una actualización disponible. */
  defaultVersion: string | null;
  availableVersions: string[];
}

export interface AvailableExtension {
  name: string;
  defaultVersion: string | null;
  installed: boolean;
  comment: string | null;
}

export const extensionPreview = (changes: ExtensionChange[]) =>
  invoke<DdlStatement[]>("extension_preview", { changes });

export const extensionApply = (id: string, changes: ExtensionChange[], database?: string) =>
  invoke<void>("extension_apply", { id, changes, database: database ?? null });

export const extensionInfo = (id: string, name: string, database?: string) =>
  invoke<ExtensionInfo>("extension_info", { id, name, database: database ?? null });

export const availableExtensions = (id: string, database?: string) =>
  invoke<AvailableExtension[]>("available_extensions", { id, database: database ?? null });

// ---------------------------------------------------------------------------
// Datos externos (wrappers, servidores foráneos, mapeos de usuario)
// ---------------------------------------------------------------------------

/** Una opción como par [clave, valor]: espeja el `(String, String)` del núcleo. */
export type FdwOption = [string, string];

/** Qué cambia de la lista de opciones: altas, cambios de valor y bajas. */
export interface OptionsDelta {
  add: FdwOption[];
  set: FdwOption[];
  drop: string[];
}

export type FdwChange =
  | {
      kind: "create";
      name: string;
      handler: string | null;
      validator: string | null;
      options: FdwOption[];
    }
  | {
      kind: "alter";
      name: string;
      handler: string | null;
      noHandler: boolean;
      validator: string | null;
      noValidator: boolean;
      options: OptionsDelta;
    }
  | { kind: "drop"; name: string; cascade: boolean };

export interface FdwInfo {
  name: string;
  handler: string | null;
  validator: string | null;
  options: FdwOption[];
  owner: string;
}

export type ServerChange =
  | {
      kind: "create";
      name: string;
      fdw: string;
      serverType: string | null;
      version: string | null;
      options: FdwOption[];
    }
  | { kind: "alter"; name: string; version: string | null; options: OptionsDelta }
  | { kind: "drop"; name: string; cascade: boolean };

export interface ServerInfo {
  name: string;
  fdw: string;
  serverType: string | null;
  version: string | null;
  options: FdwOption[];
  owner: string;
}

export type UserMappingChange =
  | { kind: "create"; server: string; user: string; options: FdwOption[] }
  | { kind: "alter"; server: string; user: string; options: OptionsDelta }
  | { kind: "drop"; server: string; user: string };

export interface UserMapping {
  user: string;
  /** `null` cuando el rol conectado no puede ver las opciones del mapeo. */
  options: FdwOption[] | null;
}

export const fdwPreview = (changes: FdwChange[]) =>
  invoke<DdlStatement[]>("fdw_preview", { changes });

export const fdwApply = (id: string, changes: FdwChange[], database?: string) =>
  invoke<void>("fdw_apply", { id, changes, database: database ?? null });

export const fdwInfo = (id: string, name: string, database?: string) =>
  invoke<FdwInfo>("fdw_info", { id, name, database: database ?? null });

export const availableFdws = (id: string, database?: string) =>
  invoke<string[]>("available_fdws", { id, database: database ?? null });

export const foreignServerPreview = (changes: ServerChange[]) =>
  invoke<DdlStatement[]>("foreign_server_preview", { changes });

export const foreignServerApply = (id: string, changes: ServerChange[], database?: string) =>
  invoke<void>("foreign_server_apply", { id, changes, database: database ?? null });

export const foreignServerInfo = (id: string, name: string, database?: string) =>
  invoke<ServerInfo>("foreign_server_info", { id, name, database: database ?? null });

export const userMappingPreview = (changes: UserMappingChange[]) =>
  invoke<DdlStatement[]>("user_mapping_preview", { changes });

export const userMappingApply = (id: string, changes: UserMappingChange[], database?: string) =>
  invoke<void>("user_mapping_apply", { id, changes, database: database ?? null });

export const userMappings = (id: string, server: string, database?: string) =>
  invoke<UserMapping[]>("user_mappings", { id, server, database: database ?? null });

// ---------------------------------------------------------------------------
// Configuración del servidor (pg_settings)
// ---------------------------------------------------------------------------

/** `bool`, `integer`, `real`, `enum` o `string`: decide el widget de edición. */
export type SettingType = "bool" | "integer" | "real" | "enum" | "string";

export interface Setting {
  name: string;
  value: string;
  unit: string | null;
  category: string;
  shortDesc: string;
  /** internal (nunca), postmaster (con reinicio), sighup (con recarga), o en caliente el resto. */
  context: string;
  varType: SettingType;
  minVal: string | null;
  maxVal: string | null;
  enumVals: string[];
  bootVal: string | null;
  resetVal: string | null;
  source: string;
  pendingRestart: boolean;
}

export type SettingChange =
  | { kind: "set"; name: string; value: string }
  | { kind: "reset"; name: string };

export const serverSettings = (id: string) => invoke<Setting[]>("server_settings", { id });

export const settingsPreview = (changes: SettingChange[]) =>
  invoke<DdlStatement[]>("settings_preview", { changes });

/** Devuelve `true` si algún cambio quedó pendiente de reinicio. */
export const settingsApply = (id: string, changes: SettingChange[]) =>
  invoke<boolean>("settings_apply", { id, changes });

// ---------------------------------------------------------------------------
// Privilegios
// ---------------------------------------------------------------------------

export type TablePrivilege =
  | "select"
  | "insert"
  | "update"
  | "delete"
  | "truncate"
  | "references"
  | "trigger";

export type ColumnPrivilege = "select" | "insert" | "update" | "references";

export type SchemaPrivilege = "usage" | "create";

export type SequencePrivilege = "usage" | "select" | "update";

export type FunctionPrivilege = "execute";

export type DatabasePrivilege = "connect" | "create" | "temporary";

export type TypePrivilege = "usage";

/**
 * Sobre qué se otorga o se revoca. El objeto y su vocabulario van juntos a propósito: así el tipo
 * no deja pedir `TRUNCATE` sobre un esquema.
 */
export type Grantable =
  | { on: "table"; schema: string; table: string; privileges: TablePrivilege[] }
  | {
      on: "columns";
      schema: string;
      table: string;
      columns: string[];
      privileges: ColumnPrivilege[];
    }
  | { on: "schema"; schema: string; privileges: SchemaPrivilege[] }
  | { on: "sequence"; schema: string; sequence: string; privileges: SequencePrivilege[] }
  | {
      on: "function";
      schema: string;
      name: string;
      /** Ya formateados por el servidor (`functionArgs`); no se reconstruyen acá. */
      args: string;
      procedure: boolean;
      privileges: FunctionPrivilege[];
    }
  | { on: "database"; database: string; privileges: DatabasePrivilege[] };

/** Sobre qué actúan los privilegios por omisión. */
export type DefaultPrivileges =
  | { on: "tables"; privileges: TablePrivilege[] }
  | { on: "sequences"; privileges: SequencePrivilege[] }
  | { on: "functions"; privileges: FunctionPrivilege[] }
  | { on: "types"; privileges: TypePrivilege[] };

/**
 * Cuándo se aplican los privilegios por omisión: solo a lo que cree `role` (por omisión, quien
 * ejecuta la sentencia) dentro de `schema` (por omisión, cualquiera).
 */
export interface DefaultScope {
  role?: string | null;
  schema?: string | null;
}

export type PrivilegeChange =
  | { kind: "grant"; target: Grantable; grantee: string; grantOption: boolean }
  | {
      kind: "revoke";
      target: Grantable;
      grantee: string;
      /** `REVOKE GRANT OPTION FOR ...`: revoca solo el permiso de volver a otorgar. */
      grantOptionOnly: boolean;
      cascade: boolean;
    }
  | {
      kind: "grantDefault";
      scope: DefaultScope;
      target: DefaultPrivileges;
      grantee: string;
      grantOption: boolean;
    }
  | {
      kind: "revokeDefault";
      scope: DefaultScope;
      target: DefaultPrivileges;
      grantee: string;
      grantOptionOnly: boolean;
      cascade: boolean;
    };

/** Un privilegio ya otorgado, tal como sale de `aclexplode`. */
export interface PrivilegeGrant {
  /** Nombre del rol, o `"PUBLIC"`. */
  grantee: string;
  /** Tal como lo devuelve el servidor: `"SELECT"`, `"INSERT"`, ... */
  privilege: string;
  grantable: boolean;
}

/** Un privilegio otorgado sobre una columna suelta. */
export interface ColumnGrant extends PrivilegeGrant {
  column: string;
}

/** Un privilegio por omisión ya definido. */
export interface DefaultGrant extends PrivilegeGrant {
  /** El rol cuyas creaciones futuras dispararán el privilegio. */
  owner: string;
  /** El esquema donde vale, o `null` si vale en todos. */
  schema: string | null;
  /** `"tables"`, `"sequences"`, `"functions"` o `"types"`. */
  objects: string;
}

export const privilegePreview = (changes: PrivilegeChange[]) =>
  invoke<DdlStatement[]>("privilege_preview", { changes });

export const privilegeApply = (id: string, changes: PrivilegeChange[], database?: string) =>
  invoke<void>("privilege_apply", { id, changes, database: database ?? null });

/** Sirve para todo lo que vive en `pg_class`: tablas, vistas, vistas materializadas y secuencias. */
export const relationPrivileges = (id: string, oid: number, database?: string) =>
  invoke<PrivilegeGrant[]>("relation_privileges", { id, oid, database: database ?? null });

export const functionPrivileges = (id: string, oid: number, database?: string) =>
  invoke<PrivilegeGrant[]>("function_privileges", { id, oid, database: database ?? null });

export const databasePrivileges = (id: string, name: string, database?: string) =>
  invoke<PrivilegeGrant[]>("database_privileges", { id, name, database: database ?? null });

export const columnPrivileges = (id: string, oid: number, database?: string) =>
  invoke<ColumnGrant[]>("column_privileges", { id, oid, database: database ?? null });

export const defaultPrivileges = (id: string, database?: string) =>
  invoke<DefaultGrant[]>("default_privileges", { id, database: database ?? null });

export const schemaPrivileges = (id: string, oid: number, database?: string) =>
  invoke<PrivilegeGrant[]>("schema_privileges", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Seguridad por fila (RLS)
// ---------------------------------------------------------------------------

export type PolicyCommand = "all" | "select" | "insert" | "update" | "delete";

/** `permissive` suma permisos (se combinan con OR); `restrictive` los recorta (con AND). */
export type PolicyKind = "permissive" | "restrictive";

export interface PolicyDef {
  command: PolicyCommand;
  kind: PolicyKind;
  /** Vacío significa PUBLIC. */
  roles: string[];
  /** Expresión SQL cruda: qué filas se ven. No vale para INSERT. */
  using: string | null;
  /** Expresión SQL cruda: qué filas se pueden escribir. No vale para SELECT ni DELETE. */
  check: string | null;
}

/** No hay «editar»: cambiar una política es borrarla y crearla de nuevo, como un trigger. */
export type PolicyChange =
  | { kind: "createPolicy"; schema: string; table: string; name: string; definition: PolicyDef }
  | { kind: "dropPolicy"; schema: string; table: string; name: string }
  /** El interruptor de la tabla. Sin esto las políticas no se aplican. */
  | { kind: "setRowSecurity"; schema: string; table: string; enabled: boolean }
  /** Que el filtro alcance también al dueño de la tabla. */
  | { kind: "setForceRowSecurity"; schema: string; table: string; forced: boolean };

export interface PolicyInfo {
  oid: number;
  name: string;
  command: PolicyCommand;
  kind: PolicyKind;
  /** Vacío significa PUBLIC. */
  roles: string[];
  using: string | null;
  check: string | null;
}

export interface TableSecurity {
  enabled: boolean;
  forced: boolean;
  policies: PolicyInfo[];
}

export const policyPreview = (changes: PolicyChange[]) =>
  invoke<DdlStatement[]>("policy_preview", { changes });

export const policyApply = (id: string, changes: PolicyChange[], database?: string) =>
  invoke<void>("policy_apply", { id, changes, database: database ?? null });

export const tableSecurity = (id: string, oid: number, database?: string) =>
  invoke<TableSecurity>("table_security", { id, oid, database: database ?? null });

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
    case "sshHostKey":
      return e.changed
        ? `La clave del host SSH ${e.host} cambió (huella ${e.fingerprint}). Podría ser un intermediario.`
        : `El host SSH ${e.host} no está verificado (huella ${e.fingerprint}).`;
    case "other":
      return e.message;
    default:
      return String(error);
  }
}

/** Devuelve el error de clave de host SSH sin verificar, o `null` si el error es de otro tipo. */
export function sshHostKey(
  error: unknown,
): { host: string; fingerprint: string; changed: boolean } | null {
  const e = error as CoreError;
  return e?.kind === "sshHostKey"
    ? { host: e.host, fingerprint: e.fingerprint, changed: e.changed }
    : null;
}
