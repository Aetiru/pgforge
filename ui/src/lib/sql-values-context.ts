/**
 * ¿El cursor está donde va un valor que se compara con una columna?
 *
 * `estado = '|`, `estado IN ('a', '|`, `c.estado LIKE '|`: ahí se pueden ofrecer los valores de la
 * columna. Se lee el texto con el tokenizador de `statement-risk.ts` (ignora comentarios y cadenas
 * de afuera) y se **resuelve la columna a una tabla**: si el nombre está en dos tablas del `FROM`,
 * o en ninguna que el catálogo conozca, se calla. Ofrecer los valores de la columna equivocada es
 * peor que no ofrecer ninguno.
 */

import type { SchemaRelation } from "./ipc";
import { tablesInScope, type TableRef } from "./sql-complete";
import { lex, type Token } from "./statement-risk";

export interface ValueContext {
  schema: string | null;
  table: string;
  column: string;
  /** El cursor está adentro de una cadena entre comillas simples. */
  insideQuote: boolean;
  /** Dónde empieza lo ya escrito del valor, sin la comilla de apertura. */
  from: number;
}

const word = (token: Token | undefined) => (token?.type === "word" ? token.value.toUpperCase() : "");
const isPunct = (token: Token | undefined, value: string) =>
  token?.type === "punct" && token.value === value;
const COMPARATORS = new Set(["=", "<", ">", "!"]);

function identName(token: Token | undefined): string | null {
  if (token?.type === "word" && !/^[0-9]/.test(token.value)) return token.value;
  if (token?.type === "opaque" && token.value.startsWith('"')) return token.inner;
  return null;
}

/** Palabras que no son el nombre de una columna aunque estén antes de un operador. */
const NOT_A_COLUMN = new Set(["NULL", "TRUE", "FALSE", "AND", "OR", "NOT", "WHERE", "ON", "WHEN", "THEN", "ELSE", "CASE"]);

/** Un valor ya escrito de una lista `IN (…)`: una cadena o un número. */
const isLiteral = (token: Token) =>
  (token.type === "opaque" && token.value.startsWith("'")) ||
  (token.type === "word" && /^[0-9]/.test(token.value));

/**
 * El índice de la columna que se compara con el valor que empieza en `at`, o `null`.
 * `at` es el índice del primer token del valor, o del lugar donde iría.
 */
function columnBefore(tokens: Token[], at: number): { qualifier: string | null; name: string } | null {
  let i = at - 1;
  const previous = tokens[i];

  if (isPunct(previous, "(")) {
    // `IN (`
    if (word(tokens[i - 1]) !== "IN") return null;
    i -= 2;
  } else if (isPunct(previous, ",")) {
    // `IN ('a', |`: se camina hacia atrás por cadenas y números hasta el paréntesis del `IN`.
    let j = i;
    while (j >= 0 && (isPunct(tokens[j], ",") || isLiteral(tokens[j]))) j -= 1;
    if (!isPunct(tokens[j], "(") || word(tokens[j - 1]) !== "IN") return null;
    i = j - 2;
  } else if (["LIKE", "ILIKE"].includes(word(previous))) {
    i -= 1;
  } else if (previous?.type === "punct" && COMPARATORS.has(previous.value)) {
    // `<>`, `!=`, `<=`: los operadores de varias letras son signos sueltos y pegados.
    while (
      i > 0 &&
      tokens[i - 1].type === "punct" &&
      COMPARATORS.has((tokens[i - 1] as { value: string }).value) &&
      tokens[i - 1].end === tokens[i].start
    ) {
      i -= 1;
    }
    // `:=` y `=>` no comparan nada.
    if (isPunct(tokens[i - 1], ":") && tokens[i - 1].end === tokens[i].start) return null;
    i -= 1;
  } else {
    return null;
  }

  // `NOT IN`, `NOT LIKE`: el `NOT` no es la columna.
  if (word(tokens[i]) === "NOT") i -= 1;

  const column = tokens[i];
  const name = identName(column);
  if (name === null || (column.type === "word" && NOT_A_COLUMN.has(column.value.toUpperCase()))) return null;
  // `columna::tipo = …` y `f(columna) = …` comparan otra cosa, no la columna.
  if (isPunct(tokens[i - 1], ":")) return null;

  if (isPunct(tokens[i - 1], ".")) {
    const qualifier = identName(tokens[i - 2]);
    // `esquema.tabla.columna` no se resuelve.
    if (qualifier === null || isPunct(tokens[i - 3], ".")) return null;
    return { qualifier, name };
  }
  return { qualifier: null, name };
}

function relationOf(relations: SchemaRelation[], ref: TableRef): SchemaRelation | undefined {
  return relations.find(
    (relation) => relation.name === ref.name && (ref.schema === null || relation.schema === ref.schema),
  );
}

export function valueContextAt(text: string, pos: number, relations: SchemaRelation[]): ValueContext | null {
  const { tokens } = lex(text);

  // ¿El cursor está adentro de una cadena de comillas simples?
  const insideAt = tokens.findIndex(
    (t) =>
      t.type === "opaque" &&
      !t.dollar &&
      t.value.startsWith("'") &&
      t.start < pos &&
      // En el borde derecho solo vale si la cadena quedó sin cerrar: `'ab|` y no `'ab'|`.
      (pos < t.end || !(t.value.length >= 2 && t.value.endsWith("'") && !t.value.endsWith("''") ) ),
  );
  // Una cadena cerrada con el cursor justo después de la comilla no es «adentro».
  const insideQuote = insideAt !== -1;

  let valueAt: number;
  let from = pos;
  if (insideQuote) {
    valueAt = insideAt;
    from = tokens[insideAt].start + 1;
  } else {
    // Un valor sin comillas a medio teclear: `estado = ac|`.
    const typing = tokens.findIndex((t) => t.type === "word" && t.start < pos && pos <= t.end);
    if (typing !== -1) {
      valueAt = typing;
      from = tokens[typing].start;
    } else {
      valueAt = tokens.findIndex((t) => t.start >= pos);
      if (valueAt === -1) valueAt = tokens.length;
    }
  }

  const column = columnBefore(tokens, valueAt);
  if (column === null) return null;

  // Qué tabla dueña de la columna: la del alias si hay calificador; si no, la única que la tenga.
  const refs = tablesInScope(text, pos);
  const owners: Array<{ ref: TableRef; relation: SchemaRelation }> = [];
  for (const ref of refs) {
    if (column.qualifier !== null && (ref.alias ?? ref.name) !== column.qualifier && ref.name !== column.qualifier) {
      continue;
    }
    const relation = relationOf(relations, ref);
    if (relation?.columns.some((c) => c.name === column.name)) owners.push({ ref, relation });
  }
  if (owners.length !== 1) return null;

  const { relation } = owners[0];
  return {
    schema: relation.schema,
    table: relation.name,
    column: column.name,
    insideQuote,
    from,
  };
}
