import { Channel } from "@tauri-apps/api/core";

import { invoke, type CoreError } from "./core";

// ---------------------------------------------------------------------------
// Monitoreo
// ---------------------------------------------------------------------------

export interface Backend {
  pid: number;
  database: string | null;
  user: string | null;
  /** `null` cuando el rol conectado no puede ver el detalle de esa sesión. */
  applicationName: string | null;
  clientAddr: string | null;
  backendType: string | null;
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

/**
 * Retraso de la replicación. `restricted` no es «sin retraso»: el rol ve que hay réplicas pero no su
 * progreso, y el núcleo lo distingue de `none` justo para que nadie lo lea como tranquilidad.
 */
export type ReplicationLag =
  | { kind: "none" }
  | { kind: "restricted" }
  | { kind: "primary"; replicas: number; seconds: number | null; bytes: number | null }
  | { kind: "standby"; seconds: number | null; bytes: number | null };

export interface Metrics {
  totalConnections: number;
  activeConnections: number;
  idleInTransaction: number;
  waitingConnections: number;
  maxConnections: number;
  transactionsPerSecond: number | null;
  cacheHitRatio: number | null;
  longestTransactionSeconds: number | null;
  /** Cuánto lleva esperando la sesión bloqueada más antigua; sobre todas las sesiones. */
  longestBlockedSeconds: number | null;
  /** Cuánto lleva la sesión más antigua en `idle in transaction`; sobre todas las sesiones. */
  longestIdleInTransactionSeconds: number | null;
  replication: ReplicationLag;
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
  /** Hace cuántos segundos se usó por última vez. Siempre `null` antes de PG 16. */
  lastScanSeconds: number | null;
  /**
   * Nunca se usó y no sostiene nada. Lo decide el núcleo: acá faltaban las guardas —la restricción
   * de un EXCLUDE, la identidad de réplica, el CLUSTER— y un índice intocable salía marcado.
   */
  unused: boolean;
  /** Por qué no se ofrece borrarlo —una restricción, la identidad de réplica, …—, o `null`. */
  protectedBy: string | null;
  /** La sentencia exacta que lo borraría; `null` si está protegido. */
  dropSql: string | null;
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
  /** `null` si la extensión perdió el texto de esta entrada; los tiempos siguen valiendo. */
  query: string | null;
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

/**
 * Desde cuándo cuentan los usos de los índices. «0 usos» sin esto engaña: el contador vuelve a cero
 * con `pg_stat_reset()` y cuando el servidor cae.
 */
export interface StatsWindow {
  /** Hace cuántos segundos se reiniciaron las estadísticas de la base; `null` si nunca a mano. */
  resetSeconds: number | null;
  /** PG 16+: el servidor anota cuándo se usó cada índice (`IndexStat.lastScanSeconds`). */
  tracksLastScan: boolean;
}

export const statsWindow = (id: string) => invoke<StatsWindow>("stats_window", { id });

/** Un índice que otro ya cubre: o es una copia, o sus columnas son el principio de las del otro. */
export interface Redundancy {
  schema: string;
  table: string;
  index: string;
  coveredBy: string;
  kind: "duplicate" | "prefix";
  bytes: number;
  scans: number;
  /** La sentencia exacta que lo borraría. */
  dropSql: string;
}

export const redundantIndexes = (id: string) =>
  invoke<Redundancy[]>("redundant_indexes", { id });

export const hasStatementStats = (id: string) => invoke<boolean>("has_statement_stats", { id });

export const statementStats = (id: string, limit?: number) =>
  invoke<StatementStat[]>("statement_stats", { id, limit: limit ?? null });

export const hasBloatStats = (id: string) => invoke<boolean>("has_bloat_stats", { id });

export const tableBloat = (id: string, limit?: number) =>
  invoke<TableBloat[]>("table_bloat", { id, limit: limit ?? null });

export const maintenancePlan = (id: string, operation: Operation, target: Target) =>
  invoke<MaintenancePlan>("maintenance_plan", { id, operation, target });

/**
 * Lanza el mantenimiento y devuelve el identificador de su proceso.
 *
 * `label` es el rótulo con el que se lo muestra —`public.pedidos`, `base app`—: lo arma la interfaz
 * porque es texto que se lee, no un dato del que dependa la operación.
 */
export const maintenanceRun = (
  id: string,
  operation: Operation,
  target: Target,
  label: string,
  database?: string,
) =>
  invoke<string>("maintenance_run", {
    id,
    operation,
    target,
    label,
    database: database ?? null,
  });

// ---------------------------------------------------------------------------
// Diagnósticos: lo que no se ve mirando una sesión
// ---------------------------------------------------------------------------

/** Qué tan grave es lo mostrado. `info` no pide hacer nada. */
export type DiagnoseLevel = "ok" | "info" | "review" | "attention";

/** Hacia dónde va una serie de muestras guardadas. */
export interface Forecast {
  /** Unidades por día (bytes por día en tamaños). */
  perDay: number;
  samples: number;
  spanDays: number;
  daysToLimit: number | null;
}

export interface SequenceUsage {
  schema: string;
  name: string;
  dataType: string;
  lastValue: number | null;
  maxValue: number;
  incrementBy: number;
  cycle: boolean;
  usedRatio: number;
  remaining: number;
  level: DiagnoseLevel;
  perDay: number | null;
  daysLeft: number | null;
}

export interface WraparoundDatabase {
  name: string;
  age: number;
  ratio: number;
  level: DiagnoseLevel;
}

export interface WraparoundTable {
  schema: string;
  table: string;
  age: number;
  ratio: number;
  level: DiagnoseLevel;
  totalBytes: number;
}

export interface Wraparound {
  freezeMaxAge: number;
  databases: WraparoundDatabase[];
  tables: WraparoundTable[];
}

export interface AutovacuumRow {
  schema: string;
  table: string;
  liveTuples: number;
  deadTuples: number;
  modifiedSinceAnalyze: number;
  vacuumAt: number;
  analyzeAt: number;
  vacuumMissing: number;
  status: "disabled" | "due" | "waiting";
  ownSettings: boolean;
  lastAutovacuumSeconds: number | null;
  lastAutoanalyzeSeconds: number | null;
  totalBytes: number;
  level: DiagnoseLevel;
}

export interface TableCache {
  schema: string;
  table: string;
  heapRead: number;
  heapHit: number;
  indexRead: number;
  indexHit: number;
  heapRatio: number | null;
  indexRatio: number | null;
  level: DiagnoseLevel;
}

export interface IndexCache {
  schema: string;
  table: string;
  index: string;
  read: number;
  hit: number;
  ratio: number | null;
  level: DiagnoseLevel;
}

export interface CacheReport {
  tables: TableCache[];
  indexes: IndexCache[];
}

export interface ConnectionGroup {
  /** `null` en application, user y address: el rol no ve el detalle de esas sesiones. */
  application: string | null;
  user: string | null;
  address: string | null;
  total: number;
  active: number;
  idle: number;
  idleInTransaction: number;
  oldestIdleSeconds: number | null;
}

export interface ConnectionReport {
  maxConnections: number;
  reserved: number;
  total: number;
  groups: ConnectionGroup[];
}

export interface SpaceEntry {
  schema: string;
  table: string;
  kind: string;
  heapBytes: number;
  toastBytes: number;
  indexBytes: number;
}

export interface SpaceMap {
  database: string;
  databaseBytes: number;
  entries: SpaceEntry[];
  restBytes: number;
}

export interface Growth {
  kind: "database" | "table";
  name: string;
  bytes: number;
  /** `null` mientras faltan muestras. */
  forecast: Forecast | null;
  projected30d: number | null;
}

export interface SpaceOverview {
  map: SpaceMap;
  growth: Growth[];
}

export interface WalReport {
  records: number;
  fullPageImages: number;
  bytes: number;
  buffersFull: number;
  resetSeconds: number | null;
  bytesPerSecond: number | null;
}

export interface CheckpointReport {
  timed: number;
  requested: number;
  forcedRatio: number | null;
  writeSeconds: number;
  syncSeconds: number;
  buffersWritten: number;
  resetSeconds: number | null;
  maxWalSizeBytes: number | null;
  checkpointTimeoutSeconds: number | null;
  completionTarget: number | null;
  /** `null` en PostgreSQL 13, que no tiene `pg_stat_wal`. */
  wal: WalReport | null;
  level: DiagnoseLevel;
  advice: string[];
}

export interface ConfigFinding {
  name: string;
  current: string;
  suggested: string | null;
  reason: string;
  level: DiagnoseLevel;
  /** La sentencia para copiar; la aplicación no la ejecuta. */
  alterSql: string | null;
  /** Hay que reiniciar el servidor; si no, alcanza con `pg_reload_conf()`. */
  restart: boolean;
}

export interface ConfigReview {
  findings: ConfigFinding[];
  /** Parámetros que no se pudieron leer por falta de permiso. */
  hidden: string[];
}

export const spaceOverview = (id: string) => invoke<SpaceOverview>("space_overview", { id });

export const sequenceUsage = (id: string, limit?: number) =>
  invoke<SequenceUsage[]>("sequence_usage", { id, limit: limit ?? null });

export const wraparoundStatus = (id: string, limit?: number) =>
  invoke<Wraparound>("wraparound_status", { id, limit: limit ?? null });

export const autovacuumStatus = (id: string, limit?: number) =>
  invoke<AutovacuumRow[]>("autovacuum_status", { id, limit: limit ?? null });

export const cacheStats = (id: string, limit?: number) =>
  invoke<CacheReport>("cache_stats", { id, limit: limit ?? null });

export const connectionReport = (id: string) => invoke<ConnectionReport>("connection_report", { id });

export const checkpointStats = (id: string) => invoke<CheckpointReport>("checkpoint_stats", { id });

/** `ramBytes` lo pone el usuario: el servidor no sabe cuánta memoria tiene la máquina. */
export const serverConfigReview = (id: string, ramBytes?: number | null) =>
  invoke<ConfigReview>("server_config_review", { id, ramBytes: ramBytes ?? null });
