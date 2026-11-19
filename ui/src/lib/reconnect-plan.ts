/**
 * Reintentos de conexión: cuánto esperar y cuándo rendirse.
 *
 * Un corte de red de unos segundos tiene que arreglarse solo; un servidor apagado o una contraseña
 * que cambió, no. La espera crece (2, 4, 8, 16 y 30 s, y de ahí cada minuto) para no martillar un
 * servidor que se está levantando, y hay un tope de intentos: pasado eso queda el botón manual de
 * siempre. Es puro y con Vitest.
 */

export const RETRY_DELAYS_SECONDS = [2, 4, 8, 16, 30, 30, 60, 60] as const;

/** Cuántos intentos se hacen antes de rendirse. */
export const MAX_ATTEMPTS = RETRY_DELAYS_SECONDS.length;

/** Espera, en segundos, antes del intento `attempt` (1 es el primero). `null` si ya no hay más. */
export function delayBefore(attempt: number): number | null {
  return RETRY_DELAYS_SECONDS[attempt - 1] ?? null;
}

/**
 * Si el error de un intento hace inútil seguir. Una contraseña rechazada (`28…`), un perfil sin
 * permiso o una clave de host SSH que cambió no se arreglan esperando, y reintentar la primera
 * puede bloquear la cuenta; la última exige una decisión de la persona.
 */
export function givesUp(error: unknown): boolean {
  const e = error as { kind?: string; code?: string } | null;
  if (!e || typeof e !== "object") return false;
  if (e.kind === "sshHostKey" || e.kind === "permission") return true;
  return e.kind === "database" && typeof e.code === "string" && e.code.startsWith("28");
}
