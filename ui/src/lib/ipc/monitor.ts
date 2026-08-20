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
