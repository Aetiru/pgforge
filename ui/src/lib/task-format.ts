/**
 * Cómo se cuenta un proceso: el rótulo de cada clase, lo que lleva corriendo y con qué terminó.
 *
 * Vive fuera de `tasks.svelte.ts` porque es lo único de la vista de procesos que se puede verificar
 * sin canal ni servidor, y es donde un error pasa desapercibido: un backup que dice «terminó» sin
 * decir cuántos bytes escribió parece igual de bien que uno que sí lo dice.
 */

import { bytes, count, duration } from "./format";

export type TaskKind = "maintenance" | "index" | "backup" | "restore" | "export" | "import";

const KIND_LABEL: Record<TaskKind, string> = {
  maintenance: "Mantenimiento",
  index: "Índice",
  backup: "Backup",
  restore: "Restore",
  export: "Exportación",
  import: "Importación",
};

export function taskKindLabel(kind: TaskKind): string {
  return KIND_LABEL[kind];
}

/** Lo que informa el evento `finished` de cada clase de proceso; lo que no aplica llega vacío. */
export interface TaskEnd {
  kind: TaskKind;
  seconds: number;
  /** Bytes escritos o leídos, en los procesos que mueven un archivo. */
  bytes?: number | null;
  rows?: number | null;
  path?: string | null;
  /** Errores que `pg_restore` decidió ignorar en vez de cortar. */
  ignoredErrors?: number | null;
}

/**
 * Con qué terminó, en una línea.
 *
 * El tiempo va siempre —es la pregunta de cualquiera que dejó algo corriendo— y lo demás solo si el
 * proceso lo informa: un `VACUUM` no escribió bytes ni filas, y decir "0 filas" sería mentir.
 */
export function outcomeText(end: TaskEnd): string {
  const parts: string[] = [];
  if (end.rows != null) parts.push(`${count(end.rows)} filas`);
  if (end.bytes != null) parts.push(bytes(end.bytes));
  if (end.ignoredErrors) parts.push(`${count(end.ignoredErrors)} errores ignorados`);
  parts.push(`en ${duration(end.seconds)}`);
  return `Terminó: ${parts.join(", ")}.`;
}

/** El avance de lo que copia un archivo, que es lo único que se sabe mientras corre. */
export function progressText(copied: number): string {
  return `${bytes(copied)} copiados`;
}

/**
 * Cuánto lleva. Mientras corre se cuenta contra ahora, así que la vista lo recalcula con un
 * intervalo propio: el proceso no manda un evento por segundo solo para mover un número.
 */
export function elapsedText(startedAt: number, finishedAt: number | null, now: number): string {
  const end = finishedAt ?? now;
  return duration(Math.max(0, (end - startedAt) / 1000));
}
