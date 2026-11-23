/**
 * Avisos sobre una sentencia que se ve bien y suele salir mal.
 *
 * Pocas reglas y todas leídas de las palabras sueltas (se reutiliza el tokenizador de
 * `statement-risk.ts`, que ya ignora comentarios, cadenas, identificadores con comillas y `$$ … $$`):
 * no es un parser. Por eso la regla de desempate es **ante la duda, callar**: un aviso de más se
 * aprende a ignorar y se lleva puesta la credibilidad de los demás, uno omitido solo le quita un
 * subrayado al usuario. Lo que no se analiza (DDL, `DO`, funciones) no recibe ningún aviso.
 *
 * El `UPDATE`/`DELETE` sin `WHERE` **no** se vuelve a detectar acá: se le pregunta a `assessRisks`,
 * que ya lo decide para la confirmación de producción, así el subrayado y la confirmación no
 * pueden discrepar.
 */

import { t } from "./i18n.svelte";
import { assessRisks, lex, type Token } from "./statement-risk";

export interface LintFinding {
  /** Posición absoluta en el documento: `offset` + índice dentro del texto de la sentencia. */
  from: number;
  to: number;
  message: string;
  rule: string;
  severity: "warning" | "info";
}

/** Solo se revisa lo que es consulta o modificación de datos. */
const ANALYZED = new Set(["SELECT", "WITH", "UPDATE", "DELETE", "INSERT", "MERGE", "VALUES"]);

/** Funciones que, aplicadas a la columna comparada, impiden usar el índice de esa columna. */
const WRAPPERS = new Set([
  "lower",
  "upper",
  "date",
  "trim",
  "btrim",
  "ltrim",
  "rtrim",
  "length",
  "abs",
  "round",
  "floor",
  "ceil",
  "ceiling",
  "md5",
]);

const AGGREGATES = new Set([
  "count",
  "sum",
  "avg",
  "min",
  "max",
  "array_agg",
  "string_agg",
  "json_agg",
  "jsonb_agg",
  "bool_and",
  "bool_or",
  "every",
]);

/** Lo que cierra la lista de condiciones de un `WHERE`. */
const ENDS_WHERE = new Set([
  "GROUP",
  "ORDER",
  "HAVING",
  "LIMIT",
  "OFFSET",
  "FETCH",
  "RETURNING",
  "UNION",
  "INTERSECT",
  "EXCEPT",
  "WINDOW",
  "FOR",
]);

const word = (token: Token | undefined) => (token?.type === "word" ? token.value.toUpperCase() : "");
const punct = (token: Token | undefined, value: string) =>
  token?.type === "punct" && token.value === value;
const COMPARATORS = new Set(["=", "<", ">", "!"]);

/** El `i`-ésimo token es una palabra que puede ser el nombre de una columna (no un literal). */
function isColumnWord(token: Token | undefined): boolean {
  if (token?.type !== "word") return false;
  if (/^[0-9]/.test(token.value)) return false;
  return !["NULL", "TRUE", "FALSE", "CURRENT_DATE", "CURRENT_TIMESTAMP", "CURRENT_TIME"].includes(
    token.value.toUpperCase(),
  );
}

/** Profundidad de paréntesis de cada token (la de un `(` es la de afuera). */
function depths(tokens: Token[]): number[] {
  let depth = 0;
  return tokens.map((token) => {
    if (punct(token, ")")) depth = Math.max(0, depth - 1);
    const here = depth;
    if (punct(token, "(")) depth += 1;
    return here;
  });
}

/** Los tramos de `SET a = …` de un `UPDATE`: ahí el `=` es una asignación y no una comparación. */
function assignmentRegions(tokens: Token[], depth: number[]): boolean[] {
  const inside = new Array<boolean>(tokens.length).fill(false);
  for (let i = 0; i < tokens.length; i += 1) {
    if (word(tokens[i]) !== "SET") continue;
    const level = depth[i];
    for (let j = i + 1; j < tokens.length; j += 1) {
      if (depth[j] < level) break;
      if (depth[j] === level && ["WHERE", "FROM", "RETURNING"].includes(word(tokens[j]))) break;
      inside[j] = true;
    }
  }
  return inside;
}

export function lintStatement(text: string, offset: number): LintFinding[] {
  const { tokens, unterminated } = lex(text);
  // Un texto a medio escribir se lee mal: mejor sin avisos que con avisos falsos.
  if (unterminated || tokens.length === 0) return [];
  if (!ANALYZED.has(word(tokens[0]))) return [];

  const depth = depths(tokens);
  const assignment = assignmentRegions(tokens, depth);
  const found: LintFinding[] = [];
  const add = (from: number, to: number, rule: string, severity: LintFinding["severity"], message: string) =>
    found.push({ from: offset + from, to: offset + to, rule, severity, message });

  // --- = NULL -------------------------------------------------------------------------------
  tokens.forEach((token, i) => {
    if (word(token) !== "NULL" || assignment[i]) return;
    const before = tokens[i - 1];
    if (before?.type !== "punct" || !COMPARATORS.has(before.value)) return;
    // Los operadores de varias letras (`<>`, `!=`, `<=`) son signos sueltos y pegados.
    let start = i - 1;
    while (
      start > 0 &&
      tokens[start - 1].type === "punct" &&
      COMPARATORS.has((tokens[start - 1] as { value: string }).value) &&
      tokens[start - 1].end === tokens[start].start
    ) {
      start -= 1;
    }
    // `:=` y `=>` (argumentos con nombre) no son comparaciones.
    if (punct(tokens[start - 1], ":") && tokens[start - 1].end === tokens[start].start) return;
    if (punct(tokens[i + 1], ">") && tokens[i + 1].start === token.end) return;
    add(
      tokens[start].start,
      token.end,
      "null-comparison",
      "warning",
      t("Comparar con NULL usando un operador nunca es verdadero: usa IS NULL o IS NOT NULL."),
    );
  });

  // --- NOT IN (SELECT …) ---------------------------------------------------------------------
  tokens.forEach((token, i) => {
    if (word(token) !== "NOT" || word(tokens[i + 1]) !== "IN" || !punct(tokens[i + 2], "(")) return;
    if (!["SELECT", "WITH"].includes(word(tokens[i + 3]))) return;
    add(
      token.start,
      tokens[i + 3].end,
      "not-in-subquery",
      "warning",
      t("NOT IN con una subconsulta devuelve vacío si alguna fila trae NULL. NOT EXISTS no tiene ese problema."),
    );
  });

  // --- LIKE '%…' -----------------------------------------------------------------------------
  tokens.forEach((token, i) => {
    if (!["LIKE", "ILIKE"].includes(word(token))) return;
    const pattern = tokens[i + 1];
    if (pattern?.type !== "opaque" || pattern.dollar || !pattern.value.startsWith("'")) return;
    // Un patrón que es solo `%` filtra todo igual y no es lo que molesta.
    if (!pattern.inner.startsWith("%") || pattern.inner === "%") return;
    add(
      pattern.start,
      pattern.end,
      "leading-wildcard",
      "info",
      t("Un patrón que empieza con % no puede usar un índice común; recorre toda la tabla. Con la extensión pg_trgm sí se puede indexar."),
    );
  });

  // --- función o cast sobre la columna, en el WHERE -----------------------------------------
  tokens.forEach((token, w) => {
    if (word(token) !== "WHERE") return;
    const level = depth[w];
    let end = tokens.length;
    for (let j = w + 1; j < tokens.length; j += 1) {
      if (depth[j] < level || (depth[j] === level && ENDS_WHERE.has(word(tokens[j])))) {
        end = j;
        break;
      }
    }
    for (let i = w + 1; i < end; i += 1) {
      const tok = tokens[i];
      const comparedAfter = (j: number) => {
        const next = tokens[j];
        if (next?.type === "punct" && COMPARATORS.has(next.value)) return true;
        return ["LIKE", "ILIKE", "IN", "BETWEEN"].includes(word(next));
      };

      // lower(col) = …  /  date_trunc('day', col) = …
      if (tok.type === "word" && punct(tokens[i + 1], "(") && !punct(tokens[i - 1], ".")) {
        const name = tok.value.toLowerCase();
        let close = -1;
        if (WRAPPERS.has(name) && isColumnWord(tokens[i + 2])) {
          let j = i + 3;
          // Columna calificada: `a.nombre`.
          while (punct(tokens[j], ".") && tokens[j + 1]?.type === "word") j += 2;
          if (punct(tokens[j], ")")) close = j;
        } else if (
          name === "date_trunc" &&
          tokens[i + 2]?.type === "opaque" &&
          punct(tokens[i + 3], ",") &&
          isColumnWord(tokens[i + 4])
        ) {
          let j = i + 5;
          while (punct(tokens[j], ".") && tokens[j + 1]?.type === "word") j += 2;
          if (punct(tokens[j], ")")) close = j;
        }
        if (close !== -1 && comparedAfter(close + 1)) {
          add(
            tok.start,
            tokens[close].end,
            "function-on-column",
            "info",
            t("{name}() sobre la columna impide usar el índice de esa columna. Se resuelve con un índice por expresión o comparando contra el valor transformado.", { name }),
          );
        }
      }

      // col::text = …
      if (
        isColumnWord(tok) &&
        punct(tokens[i + 1], ":") &&
        punct(tokens[i + 2], ":") &&
        tokens[i + 2].start === tokens[i + 1].end &&
        tokens[i + 3]?.type === "word" &&
        !punct(tokens[i - 1], ".") &&
        !punct(tokens[i - 1], ":")
      ) {
        let after = i + 4;
        // `numeric(10,2)`: el tipo lleva paréntesis.
        if (punct(tokens[after], "(")) {
          while (after < tokens.length && !punct(tokens[after], ")")) after += 1;
          after += 1;
        }
        if (comparedAfter(after)) {
          add(
            tok.start,
            tokens[i + 3].end,
            "cast-on-column",
            "info",
            t("Convertir la columna con :: impide usar su índice. Convierte el valor de la derecha en vez de la columna."),
          );
        }
      }
    }
  });

  // --- SELECT * sin LIMIT -------------------------------------------------------------------
  if (word(tokens[0]) === "SELECT") {
    let list = 1;
    const modifier = word(tokens[list]);
    const distinct = modifier === "DISTINCT";
    if (modifier === "ALL") list += 1;
    const topLevel = (i: number) => depth[i] === 0;

    const star = tokens.findIndex(
      (t, i) =>
        i >= list &&
        topLevel(i) &&
        punct(t, "*") &&
        (i === list || punct(tokens[i - 1], ",") || punct(tokens[i - 1], ".")) &&
        (punct(tokens[i + 1], ",") || word(tokens[i + 1]) === "FROM"),
    );

    const hasFrom = tokens.some((t, i) => topLevel(i) && word(t) === "FROM");
    const blocks = tokens.some((t, i) => {
      if (!topLevel(i)) return false;
      const name = word(t);
      if (["LIMIT", "FETCH", "GROUP", "UNION", "INTERSECT", "EXCEPT"].includes(name)) return true;
      return t.type === "word" && punct(tokens[i + 1], "(") && AGGREGATES.has(t.value.toLowerCase());
    });
    // Un agregado adentro de una subconsulta de la lista cuenta igual: ante la duda, callar.
    const aggregated = tokens.some(
      (t, i) => t.type === "word" && punct(tokens[i + 1], "(") && AGGREGATES.has(t.value.toLowerCase()),
    );

    if (star !== -1 && hasFrom && !distinct && !blocks && !aggregated) {
      add(
        tokens[star].start,
        tokens[star].end,
        "select-star",
        "info",
        t("SELECT * sin LIMIT trae todas las columnas de todas las filas. Si solo estás mirando, agrega LIMIT."),
      );
    }
  }

  // --- UPDATE / DELETE sin WHERE: la decide statement-risk ----------------------------------
  const risks = assessRisks(text).filter((r) => r.kind === "update-all" || r.kind === "delete-all");
  for (const risk of risks) {
    const verb = risk.kind === "update-all" ? "UPDATE" : "DELETE";
    const at = tokens.findIndex(
      (t, i) =>
        word(t) === verb &&
        depth[i] === 0 &&
        !["FOR", "ON", "DO", "KEY", "SHARE", "INSTEAD", "ALSO", "OR", "BEFORE", "AFTER", "OF"].includes(
          word(tokens[i - 1]),
        ),
    );
    if (at === -1) continue;
    add(
      tokens[at].start,
      tokens[at].end,
      risk.kind,
      "warning",
      risk.kind === "update-all"
        ? t("UPDATE sin WHERE: modifica todas las filas de la tabla.")
        : t("DELETE sin WHERE: borra todas las filas de la tabla."),
    );
  }

  // Máximo un hallazgo por regla y posición.
  const seen = new Set<string>();
  return found
    .filter((f) => {
      const key = `${f.rule}@${f.from}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .sort((a, b) => a.from - b.from);
}
