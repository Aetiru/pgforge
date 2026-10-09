import type { TableStat } from "./ipc";

/**
 * Qué decir de las estadísticas de una tabla.
 *
 * Pocas reglas y con umbrales altos: cada aviso tiene que ser algo sobre lo que se decide, y una
 * tabla chica con cien tuplas muertas no es un problema de nadie. Es puro porque equivocarse acá
 * significa mandar a alguien a correr un `VACUUM` que no hacía falta, o no avisarle del que sí.
 */
export interface TableHint {
  level: "warn" | "info";
  text: string;
}

/** Por debajo de esto, la proporción de muertas no significa nada: dos de diez es el 20 %. */
const MIN_DEAD_TUPLES = 1000;
const DEAD_RATIO = 0.2;
/** Lecturas secuenciales por cada una por índice a partir de las cuales se sospecha de un índice que falta. */
const SEQ_PER_INDEX = 10;
const MIN_ROWS_FOR_SEQ = 10_000;

export function tableHints(stat: TableStat): TableHint[] {
  const hints: TableHint[] = [];

  if (stat.deadTuples >= MIN_DEAD_TUPLES && (stat.deadRatio ?? 0) >= DEAD_RATIO) {
    hints.push({
      level: "warn",
      text: `${Math.round((stat.deadRatio ?? 0) * 100)} % de las filas están muertas: conviene un VACUUM`,
    });
  }

  // `lastAnalyzeSeconds` vacío es «nunca desde que cuentan las estadísticas», no necesariamente nunca.
  if (stat.lastAnalyzeSeconds === null && stat.liveTuples > 0) {
    hints.push({
      level: "warn",
      text: "Sin ANALYZE registrado: el planificador está estimando a ciegas",
    });
  }

  const indexScans = stat.indexScans ?? 0;
  if (
    stat.liveTuples >= MIN_ROWS_FOR_SEQ &&
    stat.sequentialScans > 0 &&
    stat.sequentialScans >= SEQ_PER_INDEX * Math.max(1, indexScans)
  ) {
    hints.push({
      level: "info",
      text: "Se la lee mucho de punta a punta: puede faltar un índice para sus consultas más frecuentes",
    });
  }

  return hints;
}

/** El último vacuum de los dos: el manual y el automático. */
export function lastVacuumSeconds(stat: TableStat): number | null {
  const values = [stat.lastVacuumSeconds, stat.lastAutovacuumSeconds].filter(
    (value): value is number => value !== null,
  );
  return values.length === 0 ? null : Math.min(...values);
}
