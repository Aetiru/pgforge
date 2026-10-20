/**
 * Filtros por columna de la grilla de datos, traducidos al `WHERE` crudo de `PageView`.
 *
 * El predicado viaja como texto (misma frontera de confianza que el editor: lo ejecuta el propio
 * usuario con sus privilegios), así que lo único que hay que garantizar acá es que **un valor
 * escrito en el menú nunca pueda salirse de su literal** ni un nombre de columna convertirse en
 * otra cosa. Por eso el nombre se valida contra la forma de la tabla y se cita siempre, y el valor
 * se escribe como literal con las comillas duplicadas. Es puro y con Vitest.
 */

export type FilterOp = "eq" | "neq" | "contains" | "isNull" | "notNull";

export const FILTER_OPS: { value: FilterOp; label: string; needsValue: boolean }[] = [
  { value: "eq", label: "es igual a", needsValue: true },
  { value: "neq", label: "es distinto de", needsValue: true },
  { value: "contains", label: "contiene", needsValue: true },
  { value: "isNull", label: "es nulo", needsValue: false },
  { value: "notNull", label: "no es nulo", needsValue: false },
];

/** Identificador entre comillas dobles, con las que tenga adentro duplicadas. */
export function quoteIdent(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

/**
 * Literal de texto. Con una barra invertida adentro se usa la forma `E'…'` y se duplica la barra:
 * así el valor significa lo mismo con `standard_conforming_strings` encendido o apagado, que es
 * una opción del servidor que acá no se conoce. Un byte nulo no puede estar en un texto de
 * PostgreSQL; se descarta en vez de cortar el literal.
 */
export function quoteLiteral(value: string): string {
  const clean = value.replace(/\u0000/g, "");
  const quoted = clean.replace(/'/g, "''");
  return clean.includes("\\") ? `E'${quoted.replace(/\\/g, "\\\\")}'` : `'${quoted}'`;
}

/** Escapa lo que `LIKE` toma como comodín, para que «50%» busque «50%» y no «50 y lo que sea». */
export function escapeLike(value: string): string {
  return value.replace(/[\\%_]/g, "\\$&");
}

/** Tipos sin operador de igualdad: se comparan como texto o el servidor rechaza el predicado. */
const AS_TEXT = new Set(["json", "xml", "point", "box", "line", "lseg", "path", "polygon", "circle"]);

export interface ColumnFilter {
  column: string;
  op: FilterOp;
  value?: string;
}

/**
 * La cláusula de una condición, o un error en español si no se puede armar.
 * `columns` son los nombres reales de la tabla y `types` (opcional, mismo orden) sus tipos.
 */
export function clause(
  filter: ColumnFilter,
  columns: readonly string[],
  types?: readonly string[],
): { sql: string } | { error: string } {
  const index = columns.indexOf(filter.column);
  if (index === -1) return { error: `La columna «${filter.column}» no existe en la tabla.` };

  const ident = quoteIdent(filter.column);
  const asText = AS_TEXT.has((types?.[index] ?? "").trim().toLowerCase());
  const left = asText ? `${ident}::text` : ident;

  switch (filter.op) {
    case "isNull":
      return { sql: `${ident} IS NULL` };
    case "notNull":
      return { sql: `${ident} IS NOT NULL` };
  }

  const value = filter.value ?? "";
  if (filter.op === "contains") {
    if (value === "") return { error: "Escribí el texto que tiene que contener." };
    // `::text` también acá: ILIKE no existe sobre números ni fechas.
    return { sql: `${ident}::text ILIKE ${quoteLiteral(`%${escapeLike(value)}%`)}` };
  }
  // `IS DISTINCT FROM` y no `<>`: con `<>` las filas con NULL desaparecerían del resultado, y
  // «distinto de 5» las incluye.
  return {
    sql:
      filter.op === "eq"
        ? `${left} = ${quoteLiteral(value)}`
        : `${left} IS DISTINCT FROM ${quoteLiteral(value)}`,
  };
}

/** Suma una condición al `WHERE` que ya había, sin que un `OR` escrito a mano la cambie de sentido. */
export function addToWhere(existing: string, condition: string): string {
  const current = existing.trim().replace(/;+\s*$/, "");
  if (current === "") return condition;
  return `(${current}) AND ${condition}`;
}

/** El texto de una condición para el diálogo, sin armar SQL. */
export function describeFilter(filter: ColumnFilter): string {
  const op = FILTER_OPS.find((item) => item.value === filter.op)?.label ?? filter.op;
  return filter.value !== undefined && FILTER_OPS.find((i) => i.value === filter.op)?.needsValue
    ? `${filter.column} ${op} «${filter.value}»`
    : `${filter.column} ${op}`;
}
