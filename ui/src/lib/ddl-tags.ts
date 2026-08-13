/**
 * Si un comando ejecutado desde el editor cambió el catálogo, y no solo los datos.
 *
 * Se mira la primera palabra de la etiqueta que devuelve PostgreSQL (`CREATE TABLE`, `DROP INDEX`,
 * `COMMENT`) y no el SQL escrito: la etiqueta la manda el servidor ya resuelta, así que no hay que
 * pelearse con comentarios, mayúsculas, `DO $$ … $$` ni con un script de veinte sentencias.
 *
 * `INSERT`, `UPDATE`, `DELETE` y `TRUNCATE` quedan afuera a propósito: cambian filas, y lo que el
 * árbol muestra son objetos. Releer el catálogo después de cada `INSERT` sería pagar una vuelta al
 * servidor por algo que no se ve.
 */
const CATALOG_TAGS = new Set([
  "CREATE",
  "DROP",
  "ALTER",
  "COMMENT",
  "GRANT",
  "REVOKE",
  "REFRESH",
  "IMPORT",
  "REASSIGN",
  // `SECURITY LABEL`.
  "SECURITY",
]);

export function changesCatalog(tag: string): boolean {
  const first = tag.trim().split(/\s+/)[0] ?? "";
  return CATALOG_TAGS.has(first.toUpperCase());
}
