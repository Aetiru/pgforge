/**
 * Cuántas filas va a tocar un `UPDATE` o un `DELETE`, contadas **antes** de ejecutarlo.
 *
 * De la sentencia se arma un `SELECT` que cuenta lo mismo que ella tocaría: la tabla, su alias, lo
 * que sigue del `FROM`/`USING` y el `WHERE`. El conteo tiene tope (`CAP`): saber que son «más de un
 * millón» alcanza para frenar, y contar cien millones de filas para eso sería peor que el daño.
 *
 * No analiza SQL: lee el texto respetando comillas, comentarios y paréntesis. Lo que no entiende
 * —un `WITH`, varias sentencias, una forma rara— devuelve `null` y la sentencia corre como siempre,
 * sin conteo. Equivocarse acá cuesta un número de más o de menos en un aviso, nunca una escritura.
 * Es puro y con Vitest.
 */

/** Hasta dónde se cuenta. */
export const CAP = 1_000_000;

export interface CountPlan {
  kind: "update" | "delete";
  /** El `SELECT count(*) …` que se ejecuta. */
  sql: string;
  /** Sin `WHERE`, toca toda la tabla. */
  hasWhere: boolean;
}

/** Texto sin comentarios, con las cadenas y los identificadores citados intactos. */
function withoutComments(sql: string): string {
  let out = "";
  let i = 0;
  let quote: string | null = null;
  while (i < sql.length) {
    const c = sql[i];
    const n = sql[i + 1];
    if (quote) {
      out += c;
      if (c === quote) quote = null;
      i++;
    } else if (c === "'" || c === '"') {
      quote = c;
      out += c;
      i++;
    } else if (c === "-" && n === "-") {
      while (i < sql.length && sql[i] !== "\n") i++;
      out += " ";
    } else if (c === "/" && n === "*") {
      const end = sql.indexOf("*/", i + 2);
      i = end === -1 ? sql.length : end + 2;
      out += " ";
    } else {
      out += c;
      i++;
    }
  }
  return out;
}

/** Posiciones de las palabras clave que están a nivel de la sentencia, fuera de cadenas y paréntesis. */
function topLevelWords(text: string): { word: string; at: number }[] {
  const found: { word: string; at: number }[] = [];
  let depth = 0;
  let quote: string | null = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === quote) quote = null;
      continue;
    }
    if (c === "'" || c === '"') quote = c;
    else if (c === "(") depth++;
    else if (c === ")") depth--;
    else if (depth === 0 && /[A-Za-z_]/.test(c) && !/[\w$]/.test(text[i - 1] ?? " ")) {
      const word = /^[A-Za-z_]\w*/.exec(text.slice(i))![0];
      found.push({ word: word.toLowerCase(), at: i });
      i += word.length - 1;
    }
  }
  return found;
}

const NAME = String.raw`(?:"(?:[^"]|"")*"|[A-Za-z_][\w$]*)(?:\s*\.\s*(?:"(?:[^"]|"")*"|[A-Za-z_][\w$]*))?`;
const ALIAS = String.raw`(?:\s+(?:as\s+)?(?!(?:set|using|where|returning|from)\b)([A-Za-z_]\w*))?`;

/** Lo que hay entre dos palabras clave de nivel superior. */
function clause(text: string, words: { word: string; at: number }[], name: string, ends: string[]): string | null {
  const start = words.find((w) => w.word === name);
  if (!start) return null;
  const end = words.find((w) => w.at > start.at && ends.includes(w.word));
  return text.slice(start.at + name.length, end?.at ?? text.length).trim();
}

export function countQuery(sql: string): CountPlan | null {
  const text = withoutComments(sql).trim().replace(/;+\s*$/, "");
  if (text === "") return null;
  // Varias sentencias: no se sabe cuál contar. Los `;` de adentro de una cadena no cuentan.
  if (text.replace(/'(?:[^']|'')*'|"(?:[^"]|"")*"/g, "").includes(";")) return null;
  const update = new RegExp(String.raw`^update\s+(?:only\s+)?(${NAME})${ALIAS}\s+set\b`, "i").exec(text);
  const del = new RegExp(String.raw`^delete\s+from\s+(?:only\s+)?(${NAME})${ALIAS}(?=\s|$)`, "i").exec(text);
  const match = update ?? del;
  if (!match) return null;

  const [head, table, aliasRaw] = match;
  const alias = aliasRaw ?? null;
  const rest = text.slice(head.length);
  // Las palabras se buscan en lo que sigue a la cabecera, con sus posiciones relativas a `rest`.
  const restWords = topLevelWords(rest);
  const source = update ? "from" : "using";
  const ends = ["where", "returning"];
  const extra = clause(rest, restWords, source, ends);
  const where = clause(rest, restWords, "where", ["returning"]);

  const relation = alias ? `${table} AS ${alias}` : table;
  const from = extra ? `${relation}, ${extra}` : relation;
  const condition = where ? ` WHERE ${where}` : "";
  return {
    kind: update ? "update" : "delete",
    sql: `SELECT count(*) FROM (SELECT 1 FROM ${from}${condition} LIMIT ${CAP + 1}) AS _n`,
    hasWhere: where !== null && where !== "",
  };
}
