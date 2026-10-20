/**
 * Qué sentencias merecen una confirmación más fuerte en producción.
 *
 * `DROP`, `TRUNCATE` y un `DELETE`/`UPDATE` sin `WHERE` son las que no se deshacen con un clic en
 * «Cancelar» del lado equivocado: la confirmación de siempre alcanza para «modificar producción»,
 * pero para estas se pide escribir el nombre del servidor.
 *
 * No hay un parser de SQL acá: se tokeniza ignorando comentarios, cadenas, identificadores citados
 * y cuerpos `$$ … $$`, y se leen las palabras que quedan. Eso alcanza para distinguir un `DROP` de
 * la palabra «drop» dentro de una cadena, y no alcanza para entender SQL arbitrario —por eso la
 * regla de desempate es **ante la duda, más fuerte, nunca más débil**: un texto sin cerrar, un
 * `DO` con SQL dinámico o un `WHERE true` se tratan como peligrosos. Un falso positivo cuesta
 * escribir un nombre; un falso negativo cuesta una tabla.
 *
 * Es el equivalente de `sql::split` del núcleo para una pregunta que solo existe en la interfaz
 * (qué confirmación pedir), así que vive acá y no se pide por IPC a cada diálogo.
 */

export type RiskKind = "drop" | "truncate" | "delete-all" | "update-all" | "unsure";

export interface StatementRisk {
  kind: RiskKind;
  /** Posición de la sentencia dentro del script, con base 0. */
  index: number;
  /** El comienzo de la sentencia, en una línea, para mostrarlo en la confirmación. */
  statement: string;
}

export const RISK_LABEL: Record<RiskKind, string> = {
  drop: "Elimina objetos (DROP)",
  truncate: "Vacía tablas enteras (TRUNCATE)",
  "delete-all": "DELETE sin WHERE: borra todas las filas",
  "update-all": "UPDATE sin WHERE: modifica todas las filas",
  unsure: "No se pudo analizar con certeza",
};

type Token =
  | { type: "word"; value: string; start: number; end: number }
  | { type: "punct"; value: string; start: number; end: number }
  /** Cadena, identificador citado o cuerpo de dólares: `inner` es lo que va adentro. */
  | { type: "opaque"; value: string; inner: string; dollar: boolean; start: number; end: number };

interface Lexed {
  tokens: Token[];
  /** Quedó una cadena, comentario o `$$` sin cerrar. */
  unterminated: boolean;
}

const WORD_CHAR = /[\p{L}\p{N}_$\u0080-￿]/u;
const WORD_START = /[\p{L}\p{N}_\u0080-￿]/u;
const DOLLAR_TAG = /^\$([\p{L}_][\p{L}\p{N}_]*)?\$/u;

function lex(sql: string): Lexed {
  const tokens: Token[] = [];
  let unterminated = false;
  let i = 0;

  while (i < sql.length) {
    const c = sql[i];
    const next = sql[i + 1];

    if (/\s/.test(c)) {
      i += 1;
    } else if (c === "-" && next === "-") {
      const end = sql.indexOf("\n", i);
      i = end === -1 ? sql.length : end + 1;
    } else if (c === "/" && next === "*") {
      // En PostgreSQL los comentarios de bloque anidan.
      let depth = 1;
      let j = i + 2;
      while (j < sql.length && depth > 0) {
        if (sql[j] === "/" && sql[j + 1] === "*") {
          depth += 1;
          j += 2;
        } else if (sql[j] === "*" && sql[j + 1] === "/") {
          depth -= 1;
          j += 2;
        } else {
          j += 1;
        }
      }
      if (depth > 0) unterminated = true;
      i = j;
    } else if (c === "'" || c === '"') {
      // `E'…'` escapa con barra invertida; la `E` quedó como palabra anterior, pegada a la comilla.
      const previous = tokens[tokens.length - 1];
      const escapes =
        c === "'" &&
        previous?.type === "word" &&
        previous.end === i &&
        previous.value.toUpperCase() === "E";
      let j = i + 1;
      let inner = "";
      let closed = false;
      while (j < sql.length) {
        if (escapes && sql[j] === "\\") {
          inner += sql.slice(j, j + 2);
          j += 2;
        } else if (sql[j] === c && sql[j + 1] === c) {
          inner += c;
          j += 2;
        } else if (sql[j] === c) {
          closed = true;
          j += 1;
          break;
        } else {
          inner += sql[j];
          j += 1;
        }
      }
      if (!closed) unterminated = true;
      tokens.push({ type: "opaque", value: sql.slice(i, j), inner, dollar: false, start: i, end: j });
      i = j;
    } else if (c === "$" && DOLLAR_TAG.test(sql.slice(i, i + 64))) {
      const tag = (DOLLAR_TAG.exec(sql.slice(i, i + 64)) as RegExpExecArray)[0];
      const close = sql.indexOf(tag, i + tag.length);
      const end = close === -1 ? sql.length : close + tag.length;
      if (close === -1) unterminated = true;
      const inner = sql.slice(i + tag.length, close === -1 ? sql.length : close);
      tokens.push({ type: "opaque", value: sql.slice(i, end), inner, dollar: true, start: i, end });
      i = end;
    } else if (WORD_START.test(c)) {
      let j = i + 1;
      while (j < sql.length && WORD_CHAR.test(sql[j])) j += 1;
      tokens.push({ type: "word", value: sql.slice(i, j), start: i, end: j });
      i = j;
    } else {
      // `$1` y cualquier otro símbolo suelto: de a uno, como el lexer del núcleo.
      tokens.push({ type: "punct", value: c, start: i, end: i + 1 });
      i += 1;
    }
  }

  return { tokens, unterminated };
}

/** Parte los tokens en sentencias por los `;` del nivel superior. */
function splitTokens(tokens: Token[]): Token[][] {
  const statements: Token[][] = [];
  let current: Token[] = [];
  for (const token of tokens) {
    if (token.type === "punct" && token.value === ";") {
      if (current.length > 0) statements.push(current);
      current = [];
    } else {
      current.push(token);
    }
  }
  if (current.length > 0) statements.push(current);
  return statements;
}

const upper = (token: Token | undefined) =>
  token?.type === "word" ? token.value.toUpperCase() : "";

const isPunct = (token: Token | undefined, value: string) =>
  token?.type === "punct" && token.value === value;

/**
 * Si el `DELETE`/`UPDATE` que empieza en `at` no tiene `WHERE` propio.
 *
 * El `WHERE` tiene que estar al mismo nivel de paréntesis: el de una subconsulta en un `USING` o en
 * un `FROM` no filtra la sentencia de afuera. El alcance termina en el `)` que cierra el paréntesis
 * donde nació, que es lo que separa a un `WITH d AS (DELETE …) SELECT …` de lo que sigue.
 */
function lacksWhere(tokens: Token[], at: number): boolean {
  let depth = 0;
  for (let j = at + 1; j < tokens.length; j += 1) {
    const token = tokens[j];
    if (isPunct(token, "(")) depth += 1;
    else if (isPunct(token, ")")) {
      if (depth === 0) break;
      depth -= 1;
    } else if (depth === 0 && upper(token) === "WHERE") {
      return isTautology(tokens, j + 1);
    }
  }
  return true;
}

/** `WHERE true` y `WHERE 1 = 1` filtran lo mismo que no tener `WHERE`. */
function isTautology(tokens: Token[], from: number): boolean {
  const ends = (index: number) => {
    const token = tokens[index];
    return token === undefined || isPunct(token, ")") || upper(token) === "RETURNING";
  };
  if (upper(tokens[from]) === "TRUE" && ends(from + 1)) return true;

  const one = (token: Token | undefined) => token?.type === "word" && token.value === "1";
  return one(tokens[from]) && isPunct(tokens[from + 1], "=") && one(tokens[from + 2]) && ends(from + 3);
}

/**
 * Palabras antes de un `UPDATE`/`DELETE` que muestran que no es una sentencia de datos: el `FOR
 * UPDATE` de un `SELECT`, el `ON UPDATE` de una clave foránea, el `DO UPDATE` de un upsert, o la
 * acción de un disparador o una regla.
 */
const NOT_DML_BEFORE = new Set(["FOR", "ON", "DO", "KEY", "SHARE", "INSTEAD", "ALSO", "OR", "BEFORE", "AFTER", "OF"]);

function riskOfTokens(tokens: Token[], relaxed: boolean): RiskKind | null {
  const first = upper(tokens[0]);
  if (first === "") return null;

  if (first === "DROP") return "drop";
  if (first === "TRUNCATE") return "truncate";
  // `ALTER TABLE … DROP COLUMN` pierde datos igual que un `DROP`, aunque no empiece con esa palabra.
  if (first === "ALTER" && tokens.some((token) => upper(token) === "DROP")) return "drop";

  // `EXPLAIN` sin `ANALYZE` no ejecuta lo que explica.
  const explainOnly =
    first === "EXPLAIN" && !tokens.some((token) => ["ANALYZE", "ANALYSE"].includes(upper(token)));
  if (explainOnly) return null;

  for (let i = 0; i < tokens.length; i += 1) {
    const word = upper(tokens[i]);
    const before = tokens[i - 1];
    const previous = upper(before);

    // Dónde puede empezar una sentencia de datos: al principio, en un paréntesis (el cuerpo de una
    // CTE) o justo después de uno (la sentencia principal que sigue a las CTE). En un bloque `DO`,
    // en cambio, cualquier lugar: plpgsql las pone detrás de `THEN`, `ELSE`, `LOOP`…
    const starts =
      i === 0 ||
      isPunct(before, "(") ||
      isPunct(before, ")") ||
      first === "EXPLAIN" ||
      (relaxed && !NOT_DML_BEFORE.has(previous));

    if (word === "DELETE" && upper(tokens[i + 1]) === "FROM" && starts && lacksWhere(tokens, i)) {
      return "delete-all";
    }
    if (
      word === "UPDATE" &&
      starts &&
      !NOT_DML_BEFORE.has(previous) &&
      lacksWhere(tokens, i)
    ) {
      return "update-all";
    }
    if (relaxed && (word === "DROP" || word === "TRUNCATE") && !NOT_DML_BEFORE.has(previous)) {
      return word === "DROP" ? "drop" : "truncate";
    }
  }

  return null;
}

/** El texto de un `DO`: el cuerpo entre dólares o la cadena. `null` si no hay. */
function doBody(tokens: Token[]): string | null {
  const body = tokens.find((token) => token.type === "opaque" && token.value[0] !== '"');
  return body?.type === "opaque" ? body.inner : null;
}

function riskOfStatement(tokens: Token[]): RiskKind | null {
  if (upper(tokens[0]) === "DO") {
    const body = doBody(tokens);
    if (body === null) return "unsure";

    const lexed = lex(body);
    // El SQL armado como cadena no se puede leer: ante la duda, lo más fuerte.
    if (lexed.unterminated || lexed.tokens.some((token) => upper(token) === "EXECUTE")) {
      return "unsure";
    }
    for (const inner of splitTokens(lexed.tokens)) {
      const risk = riskOfTokens(inner, true);
      if (risk) return risk;
    }
    return null;
  }

  return riskOfTokens(tokens, false);
}

function describe(sql: string, tokens: Token[]): string {
  const text = sql.slice(tokens[0].start, tokens[tokens.length - 1].end);
  const line = text.replace(/\s+/g, " ").trim();
  return line.length > 100 ? `${line.slice(0, 100)}…` : line;
}

/** Las sentencias del script que piden la confirmación fuerte, en el orden en que aparecen. */
export function assessRisks(sql: string): StatementRisk[] {
  const { tokens, unterminated } = lex(sql);
  const risks: StatementRisk[] = [];
  const statements = splitTokens(tokens);

  statements.forEach((statement, index) => {
    const kind = riskOfStatement(statement);
    if (kind) risks.push({ kind, index, statement: describe(sql, statement) });
  });

  if (unterminated) {
    const last = statements[statements.length - 1];
    risks.push({
      kind: "unsure",
      index: Math.max(statements.length - 1, 0),
      statement: last ? describe(sql, last) : sql.trim().slice(0, 100),
    });
  }

  return risks;
}

/** Si lo escrito alcanza para habilitar el botón: el nombre exacto, sin contar los bordes. */
export function typedMatches(typed: string, expected: string): boolean {
  return expected.trim() !== "" && typed.trim() === expected.trim();
}
