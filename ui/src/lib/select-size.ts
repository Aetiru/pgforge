/**
 * Detectar un `SELECT` sin límite sobre una sola tabla, para avisar de su tamaño en producción.
 *
 * El tope de filas de la pestaña protege la memoria de la aplicación, pero no al servidor: un
 * `SELECT * FROM eventos` sobre doscientos millones de filas igual lo hace trabajar hasta llenar el
 * tope, y uno con `ORDER BY` lo ordena entero. Acá solo se reconoce la forma simple —una tabla, sin
 * agregados, sin `LIMIT`—; lo demás no se avisa, porque un aviso que se equivoca se aprende a
 * ignorar. Es puro y con Vitest.
 */
import { tablesInScope } from "./sql-complete";

export interface SizeTarget {
  schema: string | null;
  table: string;
}

/** Texto sin comentarios ni cadenas, para que una palabra adentro de ellos no cuente. */
function plain(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/'(?:[^']|'')*'/g, "''")
    .replace(/"(?:[^"]|"")*"/g, '""');
}

/** Lo que cambia la cuenta: un `LIMIT` ya acota, y un agregado o `DISTINCT` no se acorta con uno. */
const NOT_SIMPLE =
  /\b(limit|fetch|group\s+by|distinct|union|intersect|except|window|having|for\s+update)\b|\b(count|sum|avg|min|max|array_agg|string_agg)\s*\(|\bover\s*\(/i;

export function sizeTarget(sql: string): SizeTarget | null {
  const text = plain(sql).trim().replace(/;+\s*$/, "");
  if (!/^(select|table)\b/i.test(text) || text.includes(";") || NOT_SIMPLE.test(text)) return null;

  const refs = tablesInScope(sql, 0);
  if (refs.length !== 1) return null;
  return { schema: refs[0].schema, table: refs[0].name };
}

/** La sentencia con un `LIMIT` al final, en una línea nueva para que un comentario final no lo tape. */
export function withLimit(sql: string, rows: number): string {
  const trimmed = sql.trimEnd().replace(/;+\s*$/, "");
  return `${trimmed}\nLIMIT ${rows}`;
}

/** Desde cuántas filas estimadas se avisa. */
export const BIG_TABLE_ROWS = 1_000_000;
