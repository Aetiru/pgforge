/**
 * Lo puro de la comparación de datos: qué sentencias entran en el script y cómo se muestran las
 * filas del informe.
 *
 * Como `compare-script.ts`, vive suelto porque el filtro por riesgo es del usuario y tiene que
 * rearmar el texto en el acto. El SQL lo genera Rust (`compare::data::statements`); acá solo se
 * elige cuál de las sentencias que ya vinieron se copia. El equivalente para la línea de comandos
 * es `compare::data::script`.
 */

import type { RiskFilter } from "./compare-script";
import type { DataRowDiff, DataStatement, DiffStatus, SyncRisk } from "./ipc";

export function filterDataStatements(
  statements: DataStatement[],
  filter: RiskFilter,
): DataStatement[] {
  return statements.filter((statement) => filter[statement.risk]);
}

/** El script: una sentencia por línea, igual que `compare::data::script`. */
export function dataScriptOf(statements: DataStatement[]): string {
  return statements.map((statement) => statement.sql).join("\n");
}

/** Cuántas sentencias hay de cada riesgo, para avisar qué deja afuera el filtro antes de copiar. */
export function countDataRisks(statements: DataStatement[]): Record<SyncRisk, number> {
  const counts: Record<SyncRisk, number> = { safe: 0, review: 0, destructive: 0 };
  for (const statement of statements) counts[statement.risk] += 1;
  return counts;
}

/** El estado de una fila, con el mismo vocabulario que el informe de esquemas. */
export function statusOf(row: DataRowDiff): DiffStatus {
  return row.kind;
}

/** Cómo se dibuja una celda. `null` es el NULL de la base y no se confunde con la cadena vacía. */
export function cellText(value: string | null): { text: string; isNull: boolean } {
  return value === null ? { text: "NULL", isNull: true } : { text: value, isNull: false };
}

/** La clave de una fila para mostrarla en una línea: `1 · a` para una compuesta. */
export function keyLabel(key: string[]): string {
  return key.join(" · ");
}

/** Si la columna `index` de una fila `different` es una de las que cambiaron. */
export function isChanged(row: DataRowDiff, index: number): boolean {
  return row.kind === "different" && row.changed.includes(index);
}

/** Los totales de arriba del informe: cuántas filas hay de cada clase. */
export function summary(counts: {
  onlySource: number;
  onlyTarget: number;
  different: number;
  equal: number;
}): string {
  const parts: string[] = [];
  if (counts.onlySource) parts.push(`${counts.onlySource} solo en el origen`);
  if (counts.onlyTarget) parts.push(`${counts.onlyTarget} solo en el destino`);
  if (counts.different) parts.push(`${counts.different} distintas`);
  parts.push(`${counts.equal} iguales`);
  return parts.join(" · ");
}
