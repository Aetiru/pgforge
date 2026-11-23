/**
 * SQL de arranque para una tabla: `SELECT`, `INSERT`, `UPDATE` y `DELETE` ya escritos con sus
 * columnas, para no teclear a mano la lista entera.
 *
 * Los valores que faltan van como variables `{{columna}}` (ver `query-vars.ts`): al ejecutar, la
 * pestaña pregunta cada una, así que lo generado se puede correr tal cual. Un valor de texto va
 * entre comillas en la plantilla y uno numérico o booleano no, porque la variable se reemplaza
 * **tal cual**. Es puro y con Vitest.
 */
import { t } from "./i18n.svelte";
import type { TableShape } from "./ipc";

export type GenerateKind = "select" | "insert" | "update" | "delete";

export const GENERATE_KINDS: { value: GenerateKind; label: string }[] = [
  { value: "select", label: "SELECT" },
  { value: "insert", label: "INSERT" },
  { value: "update", label: "UPDATE" },
  { value: "delete", label: "DELETE" },
];

/** Palabras que PostgreSQL no admite como identificador sin comillas. Las más comunes, no todas:
 * quien genera con una que falte recibe el error del servidor, que es claro. */
const RESERVED = new Set([
  "all", "analyse", "analyze", "and", "any", "array", "as", "asc", "both", "case", "cast", "check",
  "collate", "column", "constraint", "create", "current_date", "current_time", "current_timestamp",
  "current_user", "default", "desc", "distinct", "do", "else", "end", "except", "false", "fetch",
  "for", "foreign", "from", "grant", "group", "having", "in", "initially", "intersect", "into",
  "leading", "limit", "localtime", "localtimestamp", "not", "null", "offset", "on", "only", "or",
  "order", "placing", "primary", "references", "returning", "select", "session_user", "some",
  "symmetric", "table", "then", "to", "trailing", "true", "union", "unique", "user", "using",
  "variadic", "when", "where", "window", "with",
]);

/** El identificador tal cual se escribe: con comillas solo si hace falta. */
export function sqlIdent(name: string): string {
  return /^[a-z_][a-z0-9_$]*$/.test(name) && !RESERVED.has(name)
    ? name
    : `"${name.replace(/"/g, '""')}"`;
}

const UNQUOTED_TYPES = /^(smallint|integer|bigint|int[248]?|numeric|decimal|real|double precision|float[48]?|boolean|bool|oid|serial|bigserial)\b/;

/** El marcador de una columna: entre comillas salvo que el tipo sea numérico o booleano. */
function placeholder(name: string, typeName: string): string {
  const variable = `{{${name.replace(/[^\p{L}\p{N}_]/gu, "_").replace(/^(\d)/, "_$1")}}}`;
  return UNQUOTED_TYPES.test(typeName.trim().toLowerCase()) ? variable : `'${variable}'`;
}

export function qualifiedName(shape: Pick<TableShape, "schema" | "name">): string {
  return `${sqlIdent(shape.schema)}.${sqlIdent(shape.name)}`;
}

/** Las columnas que se pueden escribir: las que calcula el servidor quedan afuera. */
function writable(shape: TableShape) {
  return shape.columns.filter((column) => !column.generated);
}

function keyConditions(shape: TableShape): string[] {
  const names = shape.key?.columns ?? [];
  return names.map((name) => {
    const type = shape.columns.find((column) => column.name === name)?.typeName ?? "text";
    return `${sqlIdent(name)} = ${placeholder(name, type)}`;
  });
}

export function generateSql(shape: TableShape, kind: GenerateKind): string {
  const table = qualifiedName(shape);

  switch (kind) {
    case "select": {
      const list = shape.columns.map((column) => `       ${sqlIdent(column.name)}`).join(",\n");
      return `SELECT ${list.trimStart()}\nFROM ${table}\nLIMIT 100;\n`;
    }
    case "insert": {
      const columns = writable(shape);
      if (columns.length === 0) return t("-- {table} no tiene columnas que se puedan escribir.\n", { table });
      const names = columns.map((column) => sqlIdent(column.name)).join(", ");
      const values = columns.map((column) => placeholder(column.name, column.typeName)).join(", ");
      return `INSERT INTO ${table} (${names})\nVALUES (${values});\n`;
    }
    case "update": {
      const key = new Set(shape.key?.columns ?? []);
      // La clave no se cambia por omisión: es lo que señala la fila.
      const columns = writable(shape).filter((column) => !key.has(column.name));
      const conditions = keyConditions(shape);
      if (columns.length === 0) return t("-- {table} no tiene columnas distintas de la clave para actualizar.\n", { table });
      const set = columns
        .map((column) => `${sqlIdent(column.name)} = ${placeholder(column.name, column.typeName)}`)
        .join(",\n    ");
      return conditions.length === 0
        ? t("-- {table} no tiene clave: agregá el WHERE que señale la fila antes de ejecutar.\nUPDATE {table}\nSET {set}\nWHERE false;\n", { table, set })
        : `UPDATE ${table}\nSET ${set}\nWHERE ${conditions.join("\n  AND ")};\n`;
    }
    case "delete": {
      const conditions = keyConditions(shape);
      return conditions.length === 0
        ? t("-- {table} no tiene clave: agregá el WHERE que señale la fila antes de ejecutar.\nDELETE FROM {table}\nWHERE false;\n", { table })
        : `DELETE FROM ${table}\nWHERE ${conditions.join("\n  AND ")};\n`;
    }
  }
}
