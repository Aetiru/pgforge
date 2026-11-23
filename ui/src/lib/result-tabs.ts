import { t } from "./i18n.svelte";
import { count } from "./format";
import type { ResultSet } from "./query.svelte";

/**
 * Rótulo de la pestaña de un resultado, dentro del script que se acaba de ejecutar.
 *
 * Es lógica pura y no un `{#each}` con `if` adentro porque hay que probarla sin servidor: el
 * plural de «fila», el resultado sin filas de un `UPDATE`, y el truncado por el techo de la página
 * son tres formas de texto distintas para la misma pestaña.
 */
export interface ResultLabel {
  /** Va adentro de la pestaña: «#2». */
  title: string;
  /** Al lado del número, en el `.seg-count`. */
  detail: string;
  /** El `title` del botón, con la línea del script donde arrancó la sentencia. */
  hint: string;
}

export function resultLabel(set: ResultSet): ResultLabel {
  const title = `#${set.index + 1}`;
  const where = t("línea {line}", { line: set.line });

  if (set.outcome.kind === "command") {
    const detail = `${set.outcome.tag} · ${count(set.outcome.affected)}`;
    return { title, detail, hint: t("Sentencia {set} — {where} — {tag}", { set: set.index + 1, where, tag: set.outcome.tag }) };
  }

  const { rowCount, truncated } = set.outcome;
  const noun = rowCount === 1 ? t("fila") : t("filas");
  const detail = truncated ? `${count(rowCount)}+ ${noun}` : `${count(rowCount)} ${noun}`;
  return {
    title,
    detail,
    hint: t("Sentencia {set} — {where} — {count} {noun}", { set: set.index + 1, where, count: count(rowCount), noun }),
  };
}
