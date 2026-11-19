/**
 * Valores fijos de una consulta, para volverlos huecos de una abreviatura.
 *
 * Guardar «la consulta que uso todos los lunes» como abreviatura con `'2026-01-01'` y `100`
 * clavados no sirve: lo útil es que esos valores sean variables. `findLiterals` los encuentra y
 * `applyLiterals` reemplaza los que el usuario elija por `${nombre:valor}`.
 *
 * Se lee con el tokenizador de `statement-risk.ts` (comentarios, cadenas, identificadores con
 * comillas y `$$ … $$` ya vienen resueltos), y ante la duda **se calla**: un literal omitido se
 * agrega a mano, uno ofrecido de más ensucia la lista. Un texto sin cerrar no ofrece nada.
 *
 * Decisiones que no se ven en el código:
 *  - Para una **cadena**, el rango es el de **adentro** de las comillas: las comillas quedan en el
 *    cuerpo (`'${fecha:2026-01-01}'`), porque el valor que el usuario escribe en el diálogo se
 *    inserta tal cual y una variable sin comillas rompería el SQL. Por lo mismo el valor se escribe
 *    como SQL: una comilla simple adentro va duplicada (`O''Brien`). Para un número, el rango es el
 *    del número.
 *  - `LIMIT`/`OFFSET` y `FETCH FIRST n` **sí** se ofrecen (`limite`, `desde`): son justo lo que se
 *    cambia de una corrida a otra.
 *  - **No** se ofrecen los números que son estructura y no un dato: la posición de `ORDER BY 1` y
 *    `GROUP BY 2`, las medidas de un tipo (`varchar(20)`, `numeric(10,2)`), los parámetros `$1`, y
 *    las comparaciones entre constantes (`WHERE 1 = 1`).
 *  - Se saltean las cadenas con prefijo pegado (`E'…'`, `B'…'`, `X'…'`, `U&'…'`): su contenido no
 *    se lee como el SQL de siempre, y reemplazarlo cambiaría lo que significa.
 *  - Lo que ya está adentro de un `${…}` no se ofrece: es otra variable.
 */

import { lex, type Token } from "./statement-risk";
import { isReserved } from "./sql-ident";

export type LiteralKind = "string" | "number" | "date" | "timestamp";

export interface SqlLiteral {
  /** Inicio del valor en el texto: para una cadena, sin la comilla de apertura. */
  from: number;
  to: number;
  kind: LiteralKind;
  /** El valor tal como está escrito en el SQL (con las comillas duplicadas de una cadena). */
  text: string;
  /** Nombre propuesto para la variable, único dentro del resultado. */
  suggestedName: string;
}

const HOLE = /\$\{[^{}]*\}/g;
const DATE_SHAPE = /^\d{4}-\d{2}-\d{2}$/;
const TIMESTAMP_SHAPE =
  /^\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:\s*(?:Z|[+-]\d{2}(?::?\d{2})?))?$/;

/** Tipos cuya medida va entre paréntesis: lo de adentro es el tipo, no un dato. */
const TYPMOD_TYPES = new Set([
  "varchar",
  "char",
  "character",
  "varying",
  "numeric",
  "decimal",
  "bit",
  "varbit",
  "timestamp",
  "timestamptz",
  "time",
  "timetz",
  "interval",
]);

/** Palabras que fin de cláusula de `ORDER BY`/`GROUP BY` y vuelven a los números a ser datos. */
const ENDS_POSITIONAL = new Set([
  "HAVING",
  "LIMIT",
  "OFFSET",
  "FETCH",
  "UNION",
  "EXCEPT",
  "INTERSECT",
  "FOR",
  "WINDOW",
  "RETURNING",
]);

/** Palabras que se atraviesan al buscar a qué columna se compara un valor. */
const SKIP_WORDS = new Set([
  "LIKE",
  "ILIKE",
  "IN",
  "ANY",
  "ALL",
  "SOME",
  "BETWEEN",
  "AND",
  "NOT",
  "DATE",
  "TIMESTAMP",
  "TIMESTAMPTZ",
  "INTERVAL",
  "SIMILAR",
  "TO",
  "IS",
  "DISTINCT",
  "ARRAY",
  "ESCAPE",
]);

/** Palabras que no son un identificador aunque `isReserved` las deje pasar. */
const STOP_WORDS = new Set(["VALUES", "SET", "BY", "UPDATE", "DELETE", "INSERT", "RETURNING", "OVER"]);

// Sin predicado de tipo a propósito: con `value` opcional, TypeScript daría por hecho que el
// resultado falso descarta la variante entera y dejaría al resto de las ramas sin token.
const isWord = (token: Token | undefined, value?: string): boolean =>
  token?.type === "word" && (value === undefined || token.value.toUpperCase() === value);

const isPunct = (token: Token | undefined, value?: string): boolean =>
  token?.type === "punct" && (value === undefined || token.value === value);

const isNumberWord = (token: Token | undefined): boolean =>
  token?.type === "word" && /^\d+$/.test(token.value);

const OPERATOR_PUNCT = new Set(["+", "-", "*", "/", "%", "^"]);

function sanitize(name: string): string {
  const clean = name.toLowerCase().replace(/[^\p{L}\p{N}_]/gu, "_");
  return /^\d/.test(clean) ? `_${clean}` : clean;
}

/** El nombre de la columna con la que se compara el literal que está en `tokens[at]`, si se lee. */
function contextName(tokens: Token[], at: number): string | null {
  let j = at - 1;
  for (let steps = 0; j >= 0 && steps < 24; steps += 1, j -= 1) {
    const token = tokens[j];

    if (token.type === "word") {
      const upper = token.value.toUpperCase();
      if (upper === "LIMIT") return "limite";
      if (upper === "OFFSET") return "desde";
      if ((upper === "FIRST" || upper === "NEXT") && isWord(tokens[j - 1], "FETCH")) return "limite";
      if (SKIP_WORDS.has(upper)) continue;
      if (upper === "FROM" && isWord(tokens[j - 1], "DISTINCT")) continue;
      if (isNumberWord(token)) continue;
      if (isReserved(token.value) || STOP_WORDS.has(upper)) return null;
      return sanitize(token.value);
    }

    if (token.type === "opaque") {
      if (token.dollar) return null;
      // Otro valor de la misma lista se atraviesa; un identificador con comillas es la columna.
      if (token.value.startsWith("'")) continue;
      return sanitize(token.inner);
    }

    if (isPunct(token, ")")) {
      // `lower(col) = 'x'`: la columna es lo primero que se lee adentro de los paréntesis.
      let depth = 0;
      let open = -1;
      for (let k = j; k >= 0; k -= 1) {
        if (isPunct(tokens[k], ")")) depth += 1;
        else if (isPunct(tokens[k], "(")) {
          depth -= 1;
          if (depth === 0) {
            open = k;
            break;
          }
        }
      }
      if (open === -1) return null;
      for (let k = open + 1; k < j; k += 1) {
        const inner = tokens[k];
        if (inner.type === "word" && !isReserved(inner.value) && !isNumberWord(inner) && !isPunct(tokens[k + 1], "(")) {
          return sanitize(inner.value);
        }
      }
      return null;
    }

    if (token.type === "punct" && "(,=<>!~|&[".includes(token.value)) continue;
    if (token.type === "punct" && OPERATOR_PUNCT.has(token.value)) continue;
    return null;
  }
  return null;
}

/** Índices de los tokens que están adentro de la medida de un tipo: `varchar(20)`, `numeric(10,2)`. */
function typmodIndexes(tokens: Token[]): Set<number> {
  const skip = new Set<number>();
  for (let i = 0; i < tokens.length - 1; i += 1) {
    const token = tokens[i];
    if (token.type !== "word" || !TYPMOD_TYPES.has(token.value.toLowerCase())) continue;
    if (!isPunct(tokens[i + 1], "(")) continue;
    let depth = 0;
    for (let k = i + 1; k < tokens.length; k += 1) {
      if (isPunct(tokens[k], "(")) depth += 1;
      else if (isPunct(tokens[k], ")")) {
        depth -= 1;
        if (depth === 0) break;
      }
      skip.add(k);
    }
  }
  return skip;
}

export function findLiterals(sql: string): SqlLiteral[] {
  const { tokens, unterminated } = lex(sql);
  if (unterminated) return [];

  const holes = [...sql.matchAll(HOLE)].map((m) => [m.index, m.index + m[0].length] as const);
  const insideHole = (from: number, to: number) => holes.some(([a, b]) => from >= a && to <= b);
  const typmod = typmodIndexes(tokens);

  interface Found {
    from: number;
    to: number;
    kind: LiteralKind;
    name: string | null;
  }
  const found: Found[] = [];
  const consumed = new Set<number>();

  let depth = 0;
  let positionalDepth: number | null = null;

  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    const previous = tokens[i - 1];
    const next = tokens[i + 1];

    if (isPunct(token, "(")) depth += 1;
    else if (isPunct(token, ")")) {
      depth -= 1;
      if (positionalDepth !== null && depth < positionalDepth) positionalDepth = null;
    } else if (isPunct(token, ";")) positionalDepth = null;
    else if (token.type === "word") {
      const upper = token.value.toUpperCase();
      if (upper === "BY" && (isWord(previous, "ORDER") || isWord(previous, "GROUP"))) {
        positionalDepth = depth;
      } else if (positionalDepth === depth && ENDS_POSITIONAL.has(upper)) positionalDepth = null;
    }

    if (consumed.has(i) || typmod.has(i)) continue;

    if (token.type === "opaque" && !token.dollar && token.value.startsWith("'")) {
      // Prefijo pegado (`E'…'`, `B'…'`): no se lee como una cadena común.
      if (previous?.type === "word" && previous.end === token.start) continue;
      if (previous?.type === "punct" && previous.value === "&" && previous.end === token.start) continue;

      const from = token.start + 1;
      const to = token.end - 1;
      if (insideHole(token.start, token.end)) continue;
      const inner = sql.slice(from, to);

      let kind: LiteralKind = "string";
      if (isWord(previous, "DATE") || DATE_SHAPE.test(inner)) kind = "date";
      if (isWord(previous, "TIMESTAMP") || isWord(previous, "TIMESTAMPTZ") || TIMESTAMP_SHAPE.test(inner)) {
        kind = "timestamp";
      }
      // `INTERVAL '1 day'` es una cadena aunque no se parezca a una fecha: ya cae en `string`.
      found.push({ from, to, kind, name: contextName(tokens, i) });
      continue;
    }

    if (!isNumberWord(token)) continue;
    // `$1`, `t.1`: un parámetro o parte de otro nombre, no un dato.
    if (previous?.type === "punct" && previous.end === token.start && (previous.value === "$" || previous.value === ".")) continue;
    if (insideHole(token.start, token.end)) continue;

    // Decimal: `12` `.` `5` pegados son un solo número.
    let end = token.end;
    let last = i;
    if (next && isPunct(next, ".") && next.start === token.end && isNumberWord(tokens[i + 2]) && tokens[i + 2].start === next.end) {
      end = tokens[i + 2].end;
      last = i + 2;
      consumed.add(i + 1);
      consumed.add(i + 2);
    }

    // `WHERE 1 = 1`: una comparación entre constantes, no un valor para cambiar.
    const afterIndex = last + 1;
    if (isPunct(tokens[afterIndex], "=") && isNumberWord(tokens[afterIndex + 1])) {
      consumed.add(afterIndex + 1);
      continue;
    }
    if (isPunct(previous, "=") && isNumberWord(tokens[i - 2])) continue;

    // `ORDER BY 1, 2` y `GROUP BY 1`: posiciones de columna.
    if (
      positionalDepth === depth &&
      (isWord(previous, "BY") || isPunct(previous, ",")) &&
      !(tokens[afterIndex]?.type === "punct" && OPERATOR_PUNCT.has(tokens[afterIndex].value))
    ) {
      continue;
    }

    found.push({ from: token.start, to: end, kind: "number", name: contextName(tokens, i) });
  }

  // Nombres únicos: el contexto puede repetirse (`BETWEEN` repite la columna) y el diálogo
  // necesita una variable por valor.
  const used = new Set<string>();
  let fallback = 0;
  return found.map((item) => {
    const base = item.name ?? `valor${(fallback += 1)}`;
    let name = base;
    for (let k = 2; used.has(name); k += 1) name = `${base}_${k}`;
    used.add(name);
    return { from: item.from, to: item.to, kind: item.kind, text: sql.slice(item.from, item.to), suggestedName: name };
  });
}

export interface ChosenLiteral {
  from: number;
  to: number;
  name: string;
  /** El valor que queda por omisión (lo escrito en el SQL). */
  default: string;
}

/**
 * El cuerpo con cada literal elegido reemplazado por `${nombre:valor}`.
 *
 * - Dos elegidos con el mismo nombre **y** el mismo valor comparten variable; con el mismo nombre y
 *   otro valor, el segundo se llama `nombre_2` (y así), porque una variable repetida vale una sola
 *   cosa para todas sus apariciones.
 * - Un valor con `{` o `}` **no se puede** escribir como valor por omisión —`${…:}` no admite
 *   llaves adentro y el escape solo existe al insertar—: ese literal se deja como estaba.
 * - Un nombre inválido se limpia, y uno vacío pasa a `valor`.
 * - Los rangos que se pisan se descartan quedándose con el primero.
 *
 * Se reemplaza de atrás hacia adelante para no mover las posiciones que faltan.
 */
export function applyLiterals(sql: string, chosen: ChosenLiteral[]): string {
  const ordered = [...chosen]
    .filter((item) => item.from >= 0 && item.to <= sql.length && item.from <= item.to)
    .sort((a, b) => a.from - b.from);

  const usable: ChosenLiteral[] = [];
  let edge = -1;
  for (const item of ordered) {
    if (item.from < edge) continue;
    if (/[{}]/.test(item.default)) continue;
    usable.push(item);
    edge = item.to;
  }

  const defaults = new Map<string, string>();
  const final = usable.map((item) => {
    const base = sanitize(item.name.trim()) || "valor";
    let name = base;
    for (let k = 2; defaults.has(name) && defaults.get(name) !== item.default; k += 1) {
      name = `${base}_${k}`;
    }
    defaults.set(name, item.default);
    return { ...item, name };
  });

  let out = sql;
  for (const item of [...final].reverse()) {
    out = `${out.slice(0, item.from)}\${${item.name}:${item.default}}${out.slice(item.to)}`;
  }
  return out;
}
