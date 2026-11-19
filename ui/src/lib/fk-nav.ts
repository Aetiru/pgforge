/**
 * Saltos por clave foránea desde la grilla de datos.
 *
 * Un salto es un `WHERE` sobre la otra tabla. Como para `PageView` el predicado viaja crudo, lo
 * único que hay que cuidar es que un valor de celda nunca se salga de su literal: se escribe con
 * `quoteLiteral` y los nombres de columna con `quoteIdent`, igual que el filtro por columna.
 * Es puro y con Vitest.
 */
import { quoteIdent, quoteLiteral } from "./column-filter";
import type { TableLink } from "./ipc";

/** Cuántas columnas tiene la clave más larga que se ofrece: una compuesta enorme no es navegable. */
const MAX_COLUMNS = 8;

/**
 * La clave foránea de la que sale `column`, la más corta primero: con una columna que está en dos
 * claves, la simple es la que uno espera al hacer clic.
 */
export function linkFor(links: readonly TableLink[], column: string): TableLink | null {
  const candidates = links.filter(
    (link) => link.columns.includes(column) && link.columns.length <= MAX_COLUMNS,
  );
  candidates.sort((a, b) => a.columns.length - b.columns.length);
  return candidates[0] ?? null;
}

/**
 * El predicado que lleva a la fila (o filas) del otro lado, o `null` si algún valor de la clave es
 * NULL: una clave con NULL no apunta a nada, y `col = NULL` no devolvería filas de todos modos.
 *
 * `valueOf` devuelve el valor de una columna de la fila de origen.
 */
export function jumpWhere(
  link: TableLink,
  valueOf: (column: string) => string | null | undefined,
): string | null {
  if (link.columns.length === 0 || link.columns.length !== link.otherColumns.length) return null;

  const parts: string[] = [];
  for (let i = 0; i < link.columns.length; i++) {
    const value = valueOf(link.columns[i]);
    if (value === null || value === undefined) return null;
    parts.push(`${quoteIdent(link.otherColumns[i])} = ${quoteLiteral(value)}`);
  }
  return parts.join(" AND ");
}

/** Cómo se rotula el destino en un menú: `esquema.tabla` salvo que sea `public`. */
export function linkLabel(link: TableLink): string {
  return link.schema === "public" ? link.table : `${link.schema}.${link.table}`;
}
