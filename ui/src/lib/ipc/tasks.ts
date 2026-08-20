import { Channel } from "@tauri-apps/api/core";

import { invoke, type CoreError } from "./core";

// ---------------------------------------------------------------------------
// Procesos largos
//
// El mantenimiento, la creación de un índice, los backups, los restores y las copias de datos son
// la misma cosa vistos desde acá: algo que tarda, que informa mientras corre y que se puede cortar.
// Quien lleva la cuenta es Rust —el récord de cada uno vive en `process.rs`—, y la interfaz se
// engancha con un solo canal: el primer mensaje trae todo lo que hay y después llegan las
// novedades. Por eso recargar la ventana no pierde nada de lo que estaba corriendo.
// ---------------------------------------------------------------------------

export type ProcessKind = "maintenance" | "index" | "backup" | "restore" | "export" | "import";

export type ProcessStatus = "running" | "done" | "failed";

/** Con qué terminó. Lo que no aplica llega vacío: un `VACUUM` no escribió bytes ni filas. */
export interface ProcessOutcome {
  seconds: number;
  bytes: number | null;
  rows: number | null;
  /** Errores que `pg_restore` decidió ignorar en vez de cortar. */
  ignoredErrors: number | null;
  path: string | null;
  database: string | null;
}

export interface ProcessRecord {
  taskId: string;
  kind: ProcessKind;
  /** Identificador del perfil. El nombre del servidor lo resuelve la interfaz: es rótulo. */
  profile: string;
  database: string;
  target: string;
  /** El SQL o la línea de comando que se está ejecutando. */
  command: string;
  log: string[];
  /** Bytes copiados hasta ahora, en los procesos que mueven un archivo. */
  progress: number | null;
  status: ProcessStatus;
  startedMs: number;
  finishedMs: number | null;
  outcome: ProcessOutcome | null;
  error: CoreError | null;
}

export type ProcessEvent =
  | { type: "snapshot"; records: ProcessRecord[] }
  | { type: "started"; record: ProcessRecord }
  | { type: "log"; taskId: string; message: string }
  | { type: "progress"; taskId: string; bytes: number }
  | { type: "finished"; taskId: string; outcome: ProcessOutcome }
  | { type: "failed"; taskId: string; error: CoreError };

/**
 * Engancha la interfaz al registro de procesos.
 *
 * Se llama una sola vez por vida de la ventana. El primer mensaje es el estado completo, así que no
 * hace falta pedirlo aparte —y no queda una ventana entre pedir y engancharse por la que se pueda
 * perder un evento—.
 */
export const processWatch = (channel: Channel<ProcessEvent>) =>
  invoke<void>("process_watch", { channel });

/** Corta un proceso en curso, sea del servidor o un proceso hijo. */
export const processCancel = (taskId: string) => invoke<void>("process_cancel", { taskId });

/** Lo saca de la lista. Solo tiene efecto sobre los que ya terminaron. */
export const processRemove = (taskId: string) => invoke<void>("process_remove", { taskId });

export const processClear = () => invoke<void>("process_clear");
