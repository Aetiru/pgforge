/**
 * Expandir `*` a la lista de columnas.
 *
 * `SELECT * FROM clientes` se vuelve `SELECT id, nombre, … FROM clientes`, una columna por línea,
 * para sacar las que no hacen falta. Lo que **no** se expande es lo que no se puede expandir con
 * certeza: si alguna tabla de la lista no se conoce (una subconsulta, una función, una vista que el
 * catálogo todavía no trajo), expandir dejaría la lista incompleta **sin avisar**, y una consulta
 * que devuelve menos columnas de las que parece es peor que un atajo que no hace nada.
 */

import type { SchemaRelation } from "./ipc";
import { blankNested, cteRelations, tablesInScope, type TableRef } from "./sql-complete";
import { quoteIdent } from "./sql-ident";
import { lex, type Token } from "./statement-risk";

export { quoteIdent } from "./sql-ident";

export interface StarExpansion {
  from: number;
  to: number;
  insert: string;
  /** Cuántas columnas quedaron. */
  count: number;
}

const word = (token: Token | undefined) => (token?.type === "word" ? token.value.toUpperCase() : "");
const isPunct = (token: Token | undefined, value: string) =>
  token?.type === "punct" && token.value === value;

function identName(token: Token | undefined): string | null {
  if (token?.type === "word" && !/^[0-9]/.test(token.value)) return token.value;
  if (token?.type === "opaque" && token.value.startsWith('"')) return token.inner;
  return null;
}

/** Lo que, encontrado antes de un `SELECT`, muestra que el `*` está en otro lado (`WHERE a * b`). */
const NOT_A_SELECT_LIST = new Set([
  "FROM", "WHERE", "GROUP", "ORDER", "HAVING", "ON", "USING", "SET", "VALUES", "LIMIT", "OFFSET",
  "JOIN", "UNION", "INTERSECT", "EXCEPT", "WITH", "AS", "BY",
]);

function relationOf(relations: SchemaRelation[], ref: TableRef): SchemaRelation | undefined {
  return relations.find(
    (relation) => relation.name === ref.name && (ref.schema === null || relation.schema === ref.schema),
  );
}

export function expandStar(text: string, pos: number, relations: SchemaRelation[]): StarExpansion | null {
  const { tokens, unterminated } = lex(text);
  if (unterminated) return null;

  const star = tokens.findIndex((t) => isPunct(t, "*") && t.start <= pos && pos <= t.end);
  if (star === -1) return null;

  // La sentencia: lo que queda entre dos `;` de primer nivel.
  let lo = 0;
  let hi = tokens.length;
  for (let i = 0; i < star; i += 1) if (isPunct(tokens[i], ";")) lo = i + 1;
  for (let i = star + 1; i < tokens.length; i += 1) {
    if (isPunct(tokens[i], ";")) {
      hi = i;
      break;
    }
  }

  // --- qué precede al `*` ---------------------------------------------------------------------
  let first = star;
  let qualifier: string | null = null;
  const previous = tokens[star - 1];
  if (isPunct(previous, ".")) {
    const name = identName(tokens[star - 2]);
    // `esquema.tabla.*` no se resuelve: el alias o el nombre solo son los que se emparejan.
    if (name === null || isPunct(tokens[star - 3], ".")) return null;
    qualifier = name;
    first = star - 2;
  } else if (isPunct(previous, ")")) {
    // Solo `DISTINCT ON (…) *`: cualquier otro paréntesis cerrado antes de un `*` es una multiplicación.
    let depth = 0;
    let open = -1;
    for (let j = star - 1; j >= lo; j -= 1) {
      if (isPunct(tokens[j], ")")) depth += 1;
      else if (isPunct(tokens[j], "(")) {
        depth -= 1;
        if (depth === 0) {
          open = j;
          break;
        }
      }
    }
    if (open === -1 || word(tokens[open - 1]) !== "ON" || word(tokens[open - 2]) !== "DISTINCT") return null;
  } else if (!["SELECT", "DISTINCT", "ALL", "RETURNING"].includes(word(previous)) && !isPunct(previous, ",")) {
    return null;
  }

  // Lo que sigue tiene que cerrar el ítem: `2 * 3` y `a * -1` no son una lista de columnas.
  const next = tokens[star + 1];
  if (next !== undefined && next.type === "punct" && !",)".includes(next.value) && next.value !== ";") return null;
  if (next?.type === "opaque") return null;

  // --- el SELECT que la contiene ----------------------------------------------------------------
  let select = -1;
  let depth = 0;
  for (let j = first - 1; j >= lo; j -= 1) {
    const token = tokens[j];
    if (isPunct(token, ")")) depth += 1;
    else if (isPunct(token, "(")) {
      if (depth === 0) return null;
      depth -= 1;
    } else if (depth === 0 && token.type === "word") {
      const w = token.value.toUpperCase();
      if (w === "SELECT" || w === "RETURNING") {
        select = j;
        break;
      }
      if (NOT_A_SELECT_LIST.has(w)) return null;
    }
  }
  if (select === -1) return null;
  // `EXISTS (SELECT * …)`: ahí el `*` no devuelve nada que se use.
  if (word(tokens[select]) === "SELECT" && isPunct(tokens[select - 1], "(") && word(tokens[select - 2]) === "EXISTS") {
    return null;
  }

  // --- las tablas de ESE select, sin las de las subconsultas de adentro -------------------------
  let scopeEnd = hi;
  depth = 0;
  for (let j = select + 1; j < hi; j += 1) {
    if (isPunct(tokens[j], "(")) depth += 1;
    else if (isPunct(tokens[j], ")")) {
      if (depth === 0) {
        scopeEnd = j;
        break;
      }
      depth -= 1;
    }
  }
  // `RETURNING *` mira la sentencia entera: la tabla está en el `INSERT INTO`, `UPDATE` o `DELETE FROM`.
  const scopeStartToken = word(tokens[select]) === "RETURNING" ? lo : select;
  const scopeStart = tokens[scopeStartToken].start;
  const scopeText = blankNested(text.slice(scopeStart, scopeEnd < tokens.length ? tokens[scopeEnd].start : text.length));

  // Una subconsulta o una función en el FROM: sus columnas no están en el catálogo.
  depth = 0;
  for (let j = scopeStartToken; j < scopeEnd; j += 1) {
    if (isPunct(tokens[j], "(")) depth += 1;
    else if (isPunct(tokens[j], ")")) depth -= 1;
    else if (depth === 0 && ["FROM", "JOIN"].includes(word(tokens[j])) && isPunct(tokens[j + 1], "(")) return null;
  }

  const refs = tablesInScope(scopeText, pos - scopeStart);
  if (refs.length === 0) return null;
  const known = [...relations, ...cteRelations(text, pos, relations)];

  const chosen =
    qualifier === null
      ? refs
      : refs.filter((ref) => ref.alias === qualifier || (ref.alias === null && ref.name === qualifier));
  if (chosen.length === 0) return null;

  // Cualquier tabla sin columnas conocidas deja la lista incompleta: callar.
  const columns: string[] = [];
  for (const ref of chosen) {
    const relation = relationOf(known, ref);
    if (!relation || relation.columns.length === 0) return null;
    const prefix = qualifier !== null || refs.length > 1 ? quoteIdent(ref.alias ?? ref.name) : null;
    for (const column of relation.columns) {
      columns.push(prefix === null ? quoteIdent(column.name) : `${prefix}.${quoteIdent(column.name)}`);
    }
  }
  if (columns.length === 0) return null;

  // Una columna por línea, alineadas con la primera: bajo el `*` si va en la misma línea que
  // `SELECT`, o con la sangría de su línea si está solo.
  const from = tokens[first].start;
  const lineStart = text.lastIndexOf("\n", from - 1) + 1;
  const before = text.slice(lineStart, from);
  const indent = /^\s*$/.test(before) ? before : before.replace(/[^\t]/g, " ");

  return {
    from,
    to: tokens[star].end,
    insert: columns.join(`,\n${indent}`),
    count: columns.length,
  };
}
