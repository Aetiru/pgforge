/**
 * Qué sentencias entran en el script y cómo se arma su texto.
 *
 * Vive suelto y es puro porque el filtro es del usuario: destildar «lo destructivo» tiene que
 * rearmar el texto en el acto, y volver al núcleo por cada clic sería una ida y vuelta para pegar
 * cadenas. El generador de las sentencias sigue estando en Rust —acá no se escribe SQL, se elige
 * cuál de las que ya vinieron se copia—; el equivalente para la línea de comandos es
 * `compare::sync::script`.
 */

import type { DiffEntry, SyncRisk, SyncStatement } from "./ipc";

/** Qué riesgos entran. Lo destructivo arranca apagado: es lo único que puede perder datos. */
export interface RiskFilter {
  safe: boolean;
  review: boolean;
  destructive: boolean;
}

export const DEFAULT_RISKS: RiskFilter = { safe: true, review: true, destructive: false };

export function keepsRisk(filter: RiskFilter, risk: SyncRisk): boolean {
  return filter[risk];
}

export function filterStatements(
  statements: SyncStatement[],
  filter: RiskFilter,
): SyncStatement[] {
  return statements.filter((statement) => keepsRisk(filter, statement.risk));
}

/**
 * El script completo, con cada aviso como comentario arriba de su sentencia: si alguien se queda
 * solo con el texto pegado, el aviso viaja con él.
 */
export function scriptOf(statements: SyncStatement[]): string {
  return statements
    .map((statement) => (statement.note ? `-- ${statement.note}\n${statement.sql}` : statement.sql))
    .join("\n\n");
}

/** Cuántos objetos hay de cada lado, para el resumen de arriba del informe. */
export interface DiffCounts {
  onlySource: number;
  onlyTarget: number;
  different: number;
}

export function countEntries(entries: DiffEntry[]): DiffCounts {
  const counts: DiffCounts = { onlySource: 0, onlyTarget: 0, different: 0 };
  for (const entry of entries) counts[entry.status] += 1;
  return counts;
}
