import { Channel } from "@tauri-apps/api/core";

import { invoke, type CoreError } from "./core";

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