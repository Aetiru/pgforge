/**
 * Qué decir de una consulta que tarda, a partir de lo que el servidor contesta.
 *
 * `WaitInfo` viene de `sql::wait`. Lo que importa distinguir es si la consulta está **trabajando**
 * o está **parada detrás de otra sesión**: en el segundo caso hay alguien a quien mirar, y en el
 * primero esperar es lo único que hay. Es puro y con Vitest.
 */
import { t } from "./i18n.svelte";
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
  get "idle in transaction"() { return t("con la transacción abierta sin hacer nada"); },
  get "idle in transaction (aborted)"() { return t("con la transacción abortada y sin cerrar"); },
  get active() { return t("ejecutando"); },
  get idle() { return t("inactiva"); },
};

/** Cada familia de espera, dicha como la entiende quien escribió la consulta. */
const EVENT: Record<string, string> = {
  get Lock() { return t("un bloqueo"); },
  get IO() { return t("el disco"); },
  get LWLock() { return t("un recurso interno del servidor"); },
  get BufferPin() { return t("un recurso interno del servidor"); },
  get Timeout() { return t("un temporizador (un `pg_sleep`, por ejemplo)"); },
  get IPC() { return t("otro proceso del servidor"); },
};

export function describeWait(info: WaitInfo | null): WaitText | null {
  if (!info) return null;

  if (info.blocker) {
    const { pid, user, state, transactionSeconds, query } = info.blocker;
    const who = user ? `${pid} (${user})` : String(pid);
    const parts = [t("Espera un bloqueo de la sesión {who}", { who })];
    if (state && STATE[state]) parts.push(STATE[state]);
    if (transactionSeconds !== null && transactionSeconds >= 1) {
      parts.push(t("en transacción desde hace {duration}", { duration: duration(transactionSeconds) }));
    }
    const more = info.blockedBy > 1 ? t(" y otras {others}", { others: info.blockedBy - 1 }) : "";
    return {
      summary: `${parts[0]}${more}${parts.length > 1 ? " · " + parts.slice(1).join(" · ") : ""}`,
      detail: query ? query.trim().replace(/\s+/g, " ") : null,
      blocked: true,
    };
  }

  // «Client» es el servidor esperando que el cliente pida o lea algo, y «Activity» los procesos de
  // fondo: ninguno dice nada de una consulta que tarda.
  const family = info.waitEventType ? EVENT[info.waitEventType] : undefined;
  if (family) return { summary: t("Esperando {family}", { family }), detail: null, blocked: false };
  return null;
}
