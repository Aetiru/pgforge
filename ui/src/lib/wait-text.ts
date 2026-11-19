/**
 * Qué decir de una consulta que tarda, a partir de lo que el servidor contesta.
 *
 * `WaitInfo` viene de `sql::wait`. Lo que importa distinguir es si la consulta está **trabajando**
 * o está **parada detrás de otra sesión**: en el segundo caso hay alguien a quien mirar, y en el
 * primero esperar es lo único que hay. Es puro y con Vitest.
 */
import { duration } from "./format";
import type { WaitInfo } from "./ipc";

export interface WaitText {
  /** La frase corta, para el contador. */
  summary: string;
  /** Lo último que ejecutó la sesión que bloquea, si se sabe. */
  detail: string | null;
  /** Hay otra sesión que la tiene parada. */
  blocked: boolean;
}

const STATE: Record<string, string> = {
  "idle in transaction": "con la transacción abierta sin hacer nada",
  "idle in transaction (aborted)": "con la transacción abortada y sin cerrar",
  active: "ejecutando",
  idle: "inactiva",
};

/** Cada familia de espera, dicha como la entiende quien escribió la consulta. */
const EVENT: Record<string, string> = {
  Lock: "un bloqueo",
  IO: "el disco",
  LWLock: "un recurso interno del servidor",
  BufferPin: "un recurso interno del servidor",
  Timeout: "un temporizador (un `pg_sleep`, por ejemplo)",
  IPC: "otro proceso del servidor",
};

export function describeWait(info: WaitInfo | null): WaitText | null {
  if (!info) return null;

  if (info.blocker) {
    const { pid, user, state, transactionSeconds, query } = info.blocker;
    const who = user ? `${pid} (${user})` : String(pid);
    const parts = [`Espera un bloqueo de la sesión ${who}`];
    if (state && STATE[state]) parts.push(STATE[state]);
    if (transactionSeconds !== null && transactionSeconds >= 1) {
      parts.push(`en transacción desde hace ${duration(transactionSeconds)}`);
    }
    const more = info.blockedBy > 1 ? ` y otras ${info.blockedBy - 1}` : "";
    return {
      summary: `${parts[0]}${more}${parts.length > 1 ? " · " + parts.slice(1).join(" · ") : ""}`,
      detail: query ? query.trim().replace(/\s+/g, " ") : null,
      blocked: true,
    };
  }

  // «Client» es el servidor esperando que el cliente pida o lea algo, y «Activity» los procesos de
  // fondo: ninguno dice nada de una consulta que tarda.
  const family = info.waitEventType ? EVENT[info.waitEventType] : undefined;
  if (family) return { summary: `Esperando ${family}`, detail: null, blocked: false };
  return null;
}
