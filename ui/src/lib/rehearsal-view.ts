/**
 * Cómo se lee un ensayo: el veredicto, cada sentencia y los locks.
 *
 * Todo puro, para probarlo con Vitest sin montar el panel. Lo que decide el núcleo (qué corrió, qué
 * falló, qué locks hay) llega hecho; acá solo se decide cómo decirlo y en qué orden.
 */

import { t } from "./i18n.svelte";
import type { CoreError, Outcome, RehearsalLock, RehearsalReport, RehearsalStep } from "./ipc";

export type Tone = "ok" | "warn" | "bad";

export interface Verdict {
  tone: Tone;
  title: string;
  detail: string;
}

/** Lo que el `ROLLBACK` no devuelve, dicho siempre: avisarlo solo cuando se detecta lo esconde. */
export const notRevertedNote = () =>
  t("El ensayo revierte los cambios de tablas, pero no las secuencias (cada número consumido se pierde) ni nada que actúe fuera de la base.");

export function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** Los comandos cuyo recuento de filas dice algo; en un `CREATE` el cero no significa nada. */
const COUNTS_ROWS = new Set(["INSERT", "UPDATE", "DELETE", "MERGE", "COPY", "SELECT", "MOVE", "FETCH"]);

export function outcomeText(outcome: Outcome): string {
  if (outcome.kind === "rows") {
    const shown = plural(outcome.rowCount, t("fila"), t("filas"));
    return outcome.truncated ? t("{shown} (se muestran las primeras)", { shown }) : shown;
  }
  return COUNTS_ROWS.has(outcome.tag)
    ? `${outcome.tag}: ${plural(outcome.affected, t("fila"), t("filas"))}`
    : outcome.tag || t("Listo");
}

export function errorText(error: CoreError): string {
  switch (error.kind) {
    case "canceled":
      return t("Cancelada.");
    case "database":
      return error.hint ? `${error.message} (${error.hint})` : error.message;
    case "sshHostKey":
      return t("Clave del host SSH sin verificar.");
    default:
      return error.message;
  }
}

export function stepText(step: RehearsalStep): string {
  if (step.error) return errorText(step.error);
  return step.outcome ? outcomeText(step.outcome) : "";
}

/** Filas que tocó el ensayo en total (inserciones, cambios y borrados). */
export function rowsTouched(report: RehearsalReport): number {
  return report.steps.reduce((sum, step) => {
    const outcome = step.outcome;
    return outcome?.kind === "command" && COUNTS_ROWS.has(outcome.tag) && outcome.tag !== "SELECT"
      ? sum + outcome.affected
      : sum;
  }, 0);
}

export function verdict(report: RehearsalReport): Verdict {
  if (report.blocked.length > 0) {
    return {
      tone: "bad",
      title: t("No se puede ensayar este script"),
      detail: t("No se ejecutó nada. Sacá lo que se indica abajo o corré esas sentencias aparte."),
    };
  }

  if (!report.rolledBack) {
    return {
      tone: "bad",
      title: t("No se confirmó el ROLLBACK"),
      detail: report.rollbackError
        ? t("{error}. Revisá la pestaña antes de seguir: la transacción puede seguir abierta.", {
            error: report.rollbackError,
          })
        : t("Revisá la pestaña antes de seguir: la transacción puede seguir abierta."),
    };
  }

  const failed = report.steps.find((step) => step.error);
  if (failed) {
    return {
      tone: "bad",
      title: t("Falló la sentencia {index} de {total}", { index: failed.index + 1, total: report.total }),
      detail: t("Se revirtió todo. Las sentencias que seguían no se ejecutaron."),
    };
  }

  const touched = rowsTouched(report);
  return {
    tone: report.caveats.length > 0 ? "warn" : "ok",
    title: t("Salió bien y se revirtió"),
    detail: t("{statements}, {rows}. Nada quedó aplicado.", {
      statements: plural(report.steps.length, t("sentencia"), t("sentencias")),
      rows: plural(touched, t("fila tocada"), t("filas tocadas")),
    }),
  };
}

// ---------------------------------------------------------------------------
// Locks
// ---------------------------------------------------------------------------

/** De más a menos restrictivo: el que va primero es el que más molesta a los demás. */
const LOCK_ORDER = [
  "AccessExclusiveLock",
  "ExclusiveLock",
  "ShareRowExclusiveLock",
  "ShareLock",
  "ShareUpdateExclusiveLock",
  "RowExclusiveLock",
  "RowShareLock",
  "AccessShareLock",
];

const lockNotes = (): Record<string, string> => ({
  AccessExclusiveLock: t("bloquea hasta las lecturas de la tabla"),
  ExclusiveLock: t("bloquea escrituras y bloqueos de fila"),
  ShareRowExclusiveLock: t("bloquea escrituras"),
  ShareLock: t("bloquea escrituras (p. ej. CREATE INDEX sin CONCURRENTLY)"),
  ShareUpdateExclusiveLock: t("no deja correr otro VACUUM ni cambios de estructura"),
  RowExclusiveLock: t("el habitual de INSERT/UPDATE/DELETE: no estorba a las lecturas"),
  RowShareLock: t("el de SELECT … FOR UPDATE"),
  AccessShareLock: t("el de un SELECT: casi no estorba"),
});

export const lockNote = (mode: string): string => lockNotes()[mode] ?? "";

const rank = (mode: string) => {
  const index = LOCK_ORDER.indexOf(mode);
  return index === -1 ? LOCK_ORDER.length : index;
};

/** Un lock que frena a los demás lectores o escritores de la tabla, no uno de los de siempre. */
export const isHeavyLock = (mode: string): boolean => rank(mode) <= LOCK_ORDER.indexOf("ShareLock");

export interface LockGroup {
  /** `null` agrupa los que no son de una relación. */
  relation: string | null;
  modes: string[];
  heaviest: string;
  /** Alguno no se llegó a obtener: otra sesión lo tenía. */
  waiting: boolean;
}

/** Una fila por tabla con sus modos, la más restrictiva primero. */
export function groupLocks(locks: RehearsalLock[]): LockGroup[] {
  const groups = new Map<string | null, RehearsalLock[]>();
  for (const lock of locks) {
    const key = lock.relation ?? null;
    groups.set(key, [...(groups.get(key) ?? []), lock]);
  }

  return [...groups.entries()]
    .map(([relation, items]) => {
      const modes = [...new Set(items.map((lock) => lock.mode))].sort((a, b) => rank(a) - rank(b));
      return {
        relation,
        modes,
        heaviest: modes[0],
        waiting: items.some((lock) => !lock.granted),
      };
    })
    .sort(
      (a, b) =>
        rank(a.heaviest) - rank(b.heaviest) ||
        (a.relation ?? "").localeCompare(b.relation ?? ""),
    );
}
