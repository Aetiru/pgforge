/** Cuánto lleva abierta una transacción, en la unidad que se lee de un vistazo. */
export function txElapsed(sinceMs: number, nowMs: number): string {
  const seconds = Math.max(0, Math.floor((nowMs - sinceMs) / 1000));
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return `${hours} h ${minutes % 60} min`;
}

/**
 * Pasado este tiempo una transacción abierta en producción deja de ser un trámite: la sesión
 * figura «inactiva en transacción», retiene candados y frena al `VACUUM`.
 */
export const TX_STALE_SECONDS = 60;

export function txIsStale(sinceMs: number, nowMs: number): boolean {
  return nowMs - sinceMs >= TX_STALE_SECONDS * 1000;
}

/** «3 sentencias · 14 filas»: lo que se pierde si se revierte. */
export function txSummary(statements: number, rows: number): string {
  const parts = [`${statements} ${statements === 1 ? "sentencia" : "sentencias"}`];
  if (rows > 0) parts.push(`${rows} ${rows === 1 ? "fila" : "filas"}`);
  return parts.join(" · ");
}

/** El cronómetro de una ejecución en curso: décimas hasta el minuto, después minutos y segundos. */
export function runElapsed(sinceMs: number, nowMs: number): string {
  const ms = Math.max(0, nowMs - sinceMs);
  if (ms < 60_000) return `${(ms / 1000).toFixed(1).replace(".", ",")} s`;
  const total = Math.floor(ms / 1000);
  const minutes = Math.floor(total / 60);
  const seconds = String(total % 60).padStart(2, "0");
  return `${minutes}:${seconds} min`;
}
