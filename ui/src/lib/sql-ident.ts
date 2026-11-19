/**
 * Identificadores de SQL: cuándo hace falta citarlos y cuáles son palabras reservadas.
 *
 * Lo comparten el que expande `*` y el que renombra alias: los dos escriben nombres que después
 * ejecuta el servidor, y un nombre sin comillas que es una palabra reservada (o que tiene
 * mayúsculas) no significa lo que parece. La lista es la de palabras que PostgreSQL **no deja**
 * usar como nombre sin comillas (reservadas y «reservadas con función o tipo»); las demás
 * —`name`, `type`, `value`…— sí valen y citarlas de más ensuciaría el SQL.
 */

const RESERVED = new Set(
  (
    "all analyse analyze and any array as asc asymmetric authorization binary both case cast check " +
    "collate collation column concurrently constraint create cross current_catalog current_date " +
    "current_role current_schema current_time current_timestamp current_user default deferrable " +
    "desc distinct do else end except false fetch for foreign freeze from full grant group having " +
    "ilike in initially inner intersect into is isnull join lateral leading left like limit " +
    "localtime localtimestamp natural not notnull null offset on only or order outer overlaps " +
    "placing primary references returning right select session_user similar some symmetric " +
    "system_user table tablesample then to trailing true union unique user using variadic verbose " +
    "when where window with"
  ).split(" "),
);

export function isReserved(word: string): boolean {
  return RESERVED.has(word.toLowerCase());
}

/** Un identificador que el servidor lee tal cual: minúsculas, sin símbolos y no reservado. */
export function isPlainIdent(name: string): boolean {
  return /^[a-z_][a-z0-9_$]*$/.test(name) && !isReserved(name);
}

/** Cita solo cuando hace falta: mayúsculas, símbolos, espacios o palabra reservada. */
export function quoteIdent(name: string): string {
  return isPlainIdent(name) ? name : `"${name.replace(/"/g, '""')}"`;
}
