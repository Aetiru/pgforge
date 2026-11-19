/**
 * Qué consultas se pueden repetir solas.
 *
 * «Vigilar» vuelve a ejecutar la última consulta cada tanto, así que lo único que importa es que
 * repetirla no cambie nada: una lectura. La regla es **conservadora a propósito**: ante la duda no
 * se ofrece, porque una escritura repetida cada treinta segundos es el peor final posible. Una
 * columna que se llame `update` hará que no se ofrezca; es el costo de no analizar SQL de verdad.
 * Es puro y con Vitest.
 */

/** Cada cuántos segundos se puede repetir. Nada por debajo de 5: eso ya es una carga sobre el servidor. */
export const WATCH_STEPS = [5, 15, 30, 60] as const;

const READ_START = /^\s*(select|with|table|values|show)\b/i;
const WRITES = /\b(insert|update|delete|merge|create|drop|alter|truncate|call|do|copy|grant|revoke|vacuum|reindex|refresh|lock|set|nextval|setval)\b/i;

/** Quita comentarios y cadenas para que una palabra adentro de ellos no cuente. */
function stripped(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/'(?:[^']|'')*'/g, "''")
    .replace(/"(?:[^"]|"")*"/g, '""');
}

export function isWatchable(sql: string): boolean {
  const text = stripped(sql).trim().replace(/;+\s*$/, "");
  if (text === "" || text.includes(";")) return false;
  return READ_START.test(text) && !WRITES.test(text);
}
