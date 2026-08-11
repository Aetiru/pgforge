/**
 * Columnas de las tablas del `FROM`, para completar sin calificar.
 *
 * `@codemirror/lang-sql` solo ofrece columnas detrás de `tabla.` o de un alias: escrito el `FROM`,
 * un `SELECT ` sin calificar no completa nada, que es como se escribe la mayoría de las consultas.
 * Esto lo agrega **encima** de la fuente del dialecto, sin reemplazarla: las palabras clave, los
 * esquemas y `tabla.` siguen saliendo de ella.
 *
 * De qué tablas se trata se saca leyendo el texto, no el árbol de sintaxis: el árbol de un SQL a
 * medio escribir —que es cuando se completa— tiene nodos de error justo donde está el cursor.
 * Delimitar la sentencia por el `;` más cercano es una aproximación a propósito, y no una copia en
 * TypeScript del partidor del núcleo: acá una equivocación cuesta una sugerencia de más en la lista,
 * no una consulta mal mandada, y tener dos partidores de verdad es lo que se desincroniza.
 */

import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import type { SchemaRelation } from "./ipc";

/** Una tabla nombrada en el `FROM`/`JOIN`, con el alias que le hayan puesto. */
export interface TableRef {
  /** `null` cuando se escribió sin calificar (`FROM clientes`). */
  schema: string | null;
  name: string;
  alias: string | null;
}

/** Palabras que cierran la lista de tablas: lo que viene después ya no nombra una tabla. */
const ENDS_FROM = new Set([
  "where",
  "group",
  "order",
  "having",
  "limit",
  "offset",
  "union",
  "except",
  "intersect",
  "returning",
  "window",
  "fetch",
  "for",
  "on",
  "using",
  "set",
  "values",
  "join",
  "inner",
  "left",
  "right",
  "full",
  "cross",
  "natural",
  "lateral",
  // `as` no entra: es lo que **introduce** el alias, no lo que cierra la lista.
  "with",
  "select",
]);

/** Saca comentarios sin mover ninguna posición: se reemplazan por espacios, no se borran. */
function blankComments(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, (match) => " ".repeat(match.length))
    .replace(/\/\*[\s\S]*?\*\//g, (match) => match.replace(/[^\n]/g, " "));
}

/** La sentencia donde cae el cursor, delimitada por el `;` más cercano de cada lado. */
function statementAround(sql: string, cursor: number): string {
  const before = sql.lastIndexOf(";", Math.max(0, cursor - 1));
  const after = sql.indexOf(";", cursor);
  return sql.slice(before + 1, after === -1 ? sql.length : after);
}

function unquote(identifier: string): string {
  return identifier.startsWith('"') ? identifier.slice(1, -1) : identifier;
}

/** `esquema.tabla` o `tabla`, con o sin comillas. */
const NAME = String.raw`(?:"[^"]*"|[A-Za-z_À-￿][\w$À-￿]*)`;
const REFERENCE = new RegExp(
  String.raw`^(${NAME})(?:\s*\.\s*(${NAME}))?(?:\s+(?:AS\s+)?(${NAME}))?`,
  "i",
);

function parseReference(item: string): TableRef | null {
  const text = item.trim();
  // Una subconsulta o una función devuelven columnas que el catálogo no conoce.
  if (text === "" || text.startsWith("(")) return null;

  const match = REFERENCE.exec(text);
  if (!match) return null;

  const [, first, second, alias] = match;
  const schema = second ? unquote(first) : null;
  const name = unquote(second ?? first);
  if (alias && ENDS_FROM.has(alias.toLowerCase())) {
    return { schema, name, alias: null };
  }
  return { schema, name, alias: alias ? unquote(alias) : null };
}

/** Corta la lista de tablas en las comas de primer nivel, dejando las de adentro de paréntesis. */
function topLevelItems(clause: string): string[] {
  const items: string[] = [];
  let depth = 0;
  let start = 0;

  for (let at = 0; at < clause.length; at++) {
    const character = clause[at];
    if (character === "(") depth += 1;
    else if (character === ")") depth -= 1;
    else if (character === "," && depth === 0) {
      items.push(clause.slice(start, at));
      start = at + 1;
    }
  }
  items.push(clause.slice(start));
  return items;
}

/**
 * Las tablas que la sentencia del cursor tiene a la vista.
 *
 * Cubre `FROM` (con su lista separada por comas), cada `JOIN`, y el objeto de un `UPDATE` o de un
 * `INSERT INTO`, que son los tres lugares donde uno escribe columnas sin calificar.
 */
export function tablesInScope(sql: string, cursor: number): TableRef[] {
  const statement = blankComments(statementAround(sql, cursor));
  const found: TableRef[] = [];

  const clauses = /\b(from|join|update|into)\b/gi;
  let clause: RegExpExecArray | null;

  while ((clause = clauses.exec(statement)) !== null) {
    const rest = statement.slice(clause.index + clause[0].length);
    // Solo el `FROM` lleva lista; un `JOIN` nombra una tabla y sigue con `ON`.
    const items =
      clause[1].toLowerCase() === "from" ? topLevelItems(cutAtKeyword(rest)) : [cutAtKeyword(rest)];

    for (const item of items) {
      const reference = parseReference(item);
      if (reference && !found.some((seen) => same(seen, reference))) found.push(reference);
    }
  }

  return found;
}

function same(a: TableRef, b: TableRef): boolean {
  return a.schema === b.schema && a.name === b.name && a.alias === b.alias;
}

/** Recorta hasta la primera palabra que ya no puede ser parte de un nombre de tabla ni su alias. */
function cutAtKeyword(rest: string): string {
  const words = /[A-Za-z_][\w$]*/g;
  let word: RegExpExecArray | null;
  let first = true;

  while ((word = words.exec(rest)) !== null) {
    // La primera palabra es el nombre de la tabla, aunque se llame como una palabra clave.
    if (!first && ENDS_FROM.has(word[0].toLowerCase())) return rest.slice(0, word.index);
    first = false;
  }
  return rest;
}

/** Una columna ofrecida, con la tabla de la que sale. */
export interface ColumnOption {
  label: string;
  /** Qué escribir al lado en la lista: el alias si lo hay, y si no el nombre de la tabla. */
  detail: string;
}

/**
 * Las columnas de las tablas a la vista, sin repetir.
 *
 * Una columna que está en dos tablas se ofrece una vez por cada una: cuál de las dos se quiere es
 * justo lo que el `detail` contesta, y ofrecerla una sola vez escondería la ambigüedad que después
 * el servidor rechaza.
 */
export function columnOptions(relations: SchemaRelation[], refs: TableRef[]): ColumnOption[] {
  const options: ColumnOption[] = [];

  for (const ref of refs) {
    const relation = relations.find(
      (candidate) =>
        candidate.name === ref.name && (ref.schema === null || candidate.schema === ref.schema),
    );
    if (!relation) continue;

    const detail = ref.alias ?? relation.name;
    for (const column of relation.columns) {
      if (!options.some((option) => option.label === column && option.detail === detail)) {
        options.push({ label: column, detail });
      }
    }
  }

  return options;
}

/**
 * La fuente de autocompletado, para sumarla a la del dialecto.
 *
 * Las relaciones llegan por función y no por valor: el esquema se pide después de abrir la pestaña,
 * y cambia entero al cambiar de base.
 */
export function columnCompletion(relations: () => SchemaRelation[]) {
  return (context: CompletionContext): CompletionResult | null => {
    // Detrás de un punto ya contesta la fuente del dialecto, que sabe a qué tabla pertenece.
    const qualified = context.matchBefore(/\.\s*\w*/);
    if (qualified) return null;

    const word = context.matchBefore(/\w+/);
    if (!word && !context.explicit) return null;

    const sql = context.state.doc.toString();
    const options = columnOptions(relations(), tablesInScope(sql, context.pos));
    if (options.length === 0) return null;

    return {
      from: word?.from ?? context.pos,
      // Por encima de las palabras clave del dialecto: escrito el `FROM`, lo que se busca al
      // teclear tres letras es una columna de esa tabla y no `CURRENT_TIMESTAMP`.
      options: options.map((option) => ({ ...option, type: "property", boost: 1 })),
      validFor: /^\w*$/,
    };
  };
}
