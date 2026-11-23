/**
 * La insignia de actividad de un servidor en el árbol.
 *
 * Es lo que se ve de un servidor sin abrir el monitoreo: si hay sesiones paradas detrás de otra, o
 * una transacción abierta que lleva rato sin hacer nada y retiene bloqueos. Umbrales altos a
 * propósito, como en `health.ts`: una insignia que aparece por una transacción de treinta segundos
 * se aprende a ignorar. Es puro y con Vitest.
 */


import { t } from "./i18n.svelte";
/** Refleja `monitor::activity::Brief`. */
export interface ActivityBrief {
  blocked: number;
  longestIdleInTransactionSeconds: number | null;
  restricted: boolean;
}

export interface ActivityBadge {
  text: string;
  /** Qué tan grave es: decide el color. */
  level: "warn" | "bad";
  title: string;
}

/** Desde cuándo una transacción inactiva merece la insignia, y desde cuándo es grave. */
export const IDLE_WARN_SECONDS = 600;
export const IDLE_BAD_SECONDS = 3600;
/** Desde cuántas sesiones bloqueadas es grave: una sola puede ser una espera normal. */
export const BLOCKED_BAD = 5;

/** «40 min» o «2,5 h»: en una insignia angosta sobran los segundos. */
function span(seconds: number): string {
  return seconds < 5400 ? `${Math.round(seconds / 60)} min` : `${(seconds / 3600).toFixed(1).replace(".", ",")} h`;
}

/** `null` si no hay nada que decir. Sin permiso para ver las sesiones ajenas tampoco se dice nada. */
export function activityBadge(brief: ActivityBrief | undefined): ActivityBadge | null {
  if (!brief || brief.restricted) return null;

  if (brief.blocked > 0) {
    return {
      text: brief.blocked === 1 ? t("1 bloqueada") : t("{blocked} bloqueadas", { blocked: brief.blocked }),
      level: brief.blocked >= BLOCKED_BAD ? "bad" : "warn",
      title: t("Sesiones que esperan a que otra suelte un bloqueo. Abre el monitoreo para ver cuál."),
    };
  }

  const idle = brief.longestIdleInTransactionSeconds;
  if (idle !== null && idle >= IDLE_WARN_SECONDS) {
    return {
      text: t("transacción de {span}", { span: span(idle) }),
      level: idle >= IDLE_BAD_SECONDS ? "bad" : "warn",
      title:
        t("Hay una transacción abierta sin hacer nada. Retiene bloqueos y frena a VACUUM hasta que se cierre."),
    };
  }
  return null;
}
