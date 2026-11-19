/**
 * Dónde está el cursor respecto de una llamada a función.
 *
 * Alimenta la ayuda de parámetros: al abrir el paréntesis de `date_trunc(` se muestra la firma con
 * el argumento actual resaltado. Se lee del texto (tokenizador de `statement-risk.ts`: ignora
 * cadenas, comentarios y `$$ … $$`) y no del árbol de sintaxis, que tiene nodos de error justo
 * donde uno está escribiendo.
 *
 * **Qué es una llamada** y qué no: `nombre (` cuenta salvo que `nombre` sea una palabra de SQL
 * (`IN (`, `VALUES (`, `EXISTS (`…), un tipo con parámetros (`numeric(10, 2)`) o el nombre de una
 * tabla en `INSERT INTO t (` / `CREATE TABLE t (`. Ante la duda no es llamada: mostrar la firma de
 * una función que no se está llamando es peor que no mostrar nada.
 */

import { lex, type Token } from "./statement-risk";

export interface CallAt {
  schema: string | null;
  name: string;
  nameFrom: number;
  nameTo: number;
  /** Cuántas comas de primer nivel hay entre el paréntesis de apertura y el cursor. */
  argIndex: number;
}

/** Palabras que pueden ir delante de un `(` sin que sea una función. */
const NOT_A_CALL = new Set([
  "IN", "VALUES", "AS", "EXISTS", "ON", "USING", "SELECT", "FROM", "JOIN", "WHERE", "AND", "OR",
  "NOT", "ANY", "ALL", "SOME", "CASE", "WHEN", "THEN", "ELSE", "BY", "INTO", "SET", "RETURNING",
  "WITH", "UNION", "EXCEPT", "INTERSECT", "OVER", "FILTER", "WITHIN", "DISTINCT", "TABLE",
  "LATERAL", "ONLY", "ARRAY", "ROW", "LIKE", "ILIKE", "BETWEEN", "IS", "CHECK", "UNIQUE",
  "PRIMARY", "KEY", "DEFAULT", "HAVING", "OFFSET", "LIMIT", "ORDER", "GROUP", "PARTITION",
  "CONFLICT", "DO", "UPDATE", "DELETE", "INSERT", "BEGIN", "RECURSIVE", "MATERIALIZED",
]);

/** Tipos que llevan parámetros entre paréntesis: son una medida, no una función. */
const TYPE_LIKE = new Set([
  "numeric", "decimal", "varchar", "char", "character", "bit", "varbit", "timestamp",
  "timestamptz", "time", "timetz", "interval", "float", "double",
]);

/** Si va antes del nombre, lo que sigue es un objeto que se define o se nombra, no una llamada. */
const OBJECT_BEFORE = new Set(["INTO", "TABLE", "VIEW", "INDEX", "TYPE", "FUNCTION", "PROCEDURE", "TRIGGER", "REFERENCES"]);

const upper = (token: Token | undefined) => (token?.type === "word" ? token.value.toUpperCase() : "");
const isPunct = (token: Token | undefined, value: string) =>
  token?.type === "punct" && token.value === value;

/** El nombre de un identificador, con o sin comillas. */
function identOf(token: Token | undefined): string | null {
  if (token?.type === "word") return token.value;
  if (token?.type === "opaque" && token.value.startsWith('"')) return token.inner;
  return null;
}

interface Frame {
  open: number;
  commas: number;
}

export function callAt(text: string, pos: number): CallAt | null {
  const { tokens } = lex(text);
  const stack: Frame[] = [];
  let statementStart = 0;

  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token.start >= pos) break;
    if (isPunct(token, ";")) {
      stack.length = 0;
      statementStart = i + 1;
    } else if (isPunct(token, "(")) {
      stack.push({ open: i, commas: 0 });
    } else if (isPunct(token, ")")) {
      stack.pop();
    } else if (isPunct(token, ",") && stack.length > 0) {
      stack[stack.length - 1].commas += 1;
    }
  }

  const first = upper(tokens[statementStart]);

  for (let level = stack.length - 1; level >= 0; level -= 1) {
    const frame = stack[level];
    const nameToken = tokens[frame.open - 1];
    const name = identOf(nameToken);
    // El paréntesis tiene que estar pegado a un nombre.
    if (name === null || nameToken === undefined) continue;
    const isQuoted = nameToken.type === "opaque";
    if (!isQuoted && NOT_A_CALL.has(nameToken.value.toUpperCase())) continue;
    if (!isQuoted && TYPE_LIKE.has(nameToken.value.toLowerCase())) continue;

    // `esquema.nombre(`
    let schema: string | null = null;
    let before = frame.open - 2;
    if (isPunct(tokens[before], ".") && identOf(tokens[before - 1]) !== null) {
      schema = identOf(tokens[before - 1]);
      before -= 2;
    }

    const lead = upper(tokens[before]);
    if (OBJECT_BEFORE.has(lead)) continue;
    // `CREATE INDEX i ON t (a, b)`: el nombre es la tabla; en un `JOIN … ON f(x)` sí es llamada.
    if (lead === "ON" && ["CREATE", "ALTER", "DROP"].includes(first)) continue;
    // La lista de columnas o la definición de la función que se está creando.
    if (first === "CREATE" && level === 0 && ["FUNCTION", "PROCEDURE"].includes(upper(tokens[before]))) continue;

    return {
      schema,
      name,
      nameFrom: nameToken.start,
      nameTo: nameToken.end,
      argIndex: frame.commas,
    };
  }
  return null;
}

/**
 * Parte la lista de argumentos de `pg_get_function_arguments` en parámetros.
 *
 * La coma de `numeric(10,2)` o de un `DEFAULT 'a,b'` no separa parámetros, así que se respetan los
 * paréntesis, los corchetes y las comillas. Con un `VARIADIC` el último parámetro recibe todos los
 * argumentos que sobran, y por eso el índice activo no pasa de él.
 */
export function activeParameter(args: string, index: number): { parts: string[]; activeIndex: number } {
  const parts: string[] = [];
  let depth = 0;
  let quote: string | null = null;
  let start = 0;

  for (let i = 0; i < args.length; i += 1) {
    const c = args[i];
    if (quote) {
      if (c === quote && args[i + 1] === quote) i += 1;
      else if (c === quote) quote = null;
    } else if (c === "'" || c === '"') quote = c;
    else if (c === "(" || c === "[") depth += 1;
    else if (c === ")" || c === "]") depth -= 1;
    else if (c === "," && depth === 0) {
      parts.push(args.slice(start, i).trim());
      start = i + 1;
    }
  }
  const last = args.slice(start).trim();
  if (last !== "" || parts.length > 0) parts.push(last);
  if (parts.length === 1 && parts[0] === "") return { parts: [], activeIndex: 0 };

  const variadic = parts.findIndex((part) => /^variadic\s/i.test(part));
  const activeIndex =
    variadic !== -1 && index >= variadic ? variadic : Math.max(0, Math.min(index, parts.length - 1));
  return { parts, activeIndex };
}
