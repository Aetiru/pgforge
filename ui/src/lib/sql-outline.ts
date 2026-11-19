/**
 * Las sentencias de un script, para navegarlo y para «ejecutar hasta/desde el cursor».
 *
 * **Es un partidor de navegación.** El que decide qué se ejecuta de verdad sigue siendo `sql::split`
 * del núcleo, y por eso lo que se manda a correr desde acá (`rangeUntil`/`rangeFrom`) es un
 * **texto** que el núcleo vuelve a partir: si los dos discreparan en un borde, se ejecutaría lo que
 * el núcleo entiende, nunca una sentencia cortada a medias por este. Acá no se tokeniza con
 * `statement-risk.ts` porque este necesita **ver los comentarios** —un `-- título` arriba de la
 * sentencia es su nombre en la lista—, que el otro descarta.
 *
 * Respeta `$$ … $$` y `$etiqueta$`, comillas, comentarios de bloque anidados y los cuerpos
 * `BEGIN ATOMIC … END` de una función, donde los `;` no cierran la sentencia.
 */

export interface OutlineItem {
  from: number;
  to: number;
  /** Con base 1. */
  line: number;
  kind: string;
  title: string;
}

type Piece =
  | { type: "word"; value: string; start: number; end: number }
  | { type: "punct"; value: string; start: number; end: number }
  | { type: "other"; value: string; start: number; end: number }
  | { type: "comment"; value: string; start: number; end: number };

const WORD_START = /[\p{L}_\u0080-￿]/u;
const WORD_CHAR = /[\p{L}\p{N}_$\u0080-￿]/u;
const DOLLAR_TAG = /^\$([\p{L}_][\p{L}\p{N}_]*)?\$/u;

function scan(text: string): Piece[] {
  const pieces: Piece[] = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    const next = text[i + 1];
    if (/\s/.test(c)) {
      i += 1;
    } else if (c === "-" && next === "-") {
      const nl = text.indexOf("\n", i);
      const end = nl === -1 ? text.length : nl;
      pieces.push({ type: "comment", value: text.slice(i, end), start: i, end });
      i = end;
    } else if (c === "/" && next === "*") {
      let depth = 1;
      let j = i + 2;
      while (j < text.length && depth > 0) {
        if (text[j] === "/" && text[j + 1] === "*") {
          depth += 1;
          j += 2;
        } else if (text[j] === "*" && text[j + 1] === "/") {
          depth -= 1;
          j += 2;
        } else j += 1;
      }
      pieces.push({ type: "comment", value: text.slice(i, j), start: i, end: j });
      i = j;
    } else if (c === "'" || c === '"') {
      const previous = pieces[pieces.length - 1];
      const escapes = c === "'" && previous?.type === "word" && previous.end === i && /^e$/i.test(previous.value);
      let j = i + 1;
      while (j < text.length) {
        if (escapes && text[j] === "\\") j += 2;
        else if (text[j] === c && text[j + 1] === c) j += 2;
        else if (text[j] === c) {
          j += 1;
          break;
        } else j += 1;
      }
      pieces.push({ type: "other", value: text.slice(i, j), start: i, end: j });
      i = j;
    } else if (c === "$" && DOLLAR_TAG.test(text.slice(i, i + 64))) {
      const tag = (DOLLAR_TAG.exec(text.slice(i, i + 64)) as RegExpExecArray)[0];
      const close = text.indexOf(tag, i + tag.length);
      const end = close === -1 ? text.length : close + tag.length;
      pieces.push({ type: "other", value: text.slice(i, end), start: i, end });
      i = end;
    } else if (WORD_START.test(c) || /\d/.test(c)) {
      let j = i + 1;
      while (j < text.length && WORD_CHAR.test(text[j])) j += 1;
      pieces.push({ type: "word", value: text.slice(i, j), start: i, end: j });
      i = j;
    } else {
      pieces.push({ type: "punct", value: c, start: i, end: i + 1 });
      i += 1;
    }
  }
  return pieces;
}

const upper = (piece: Piece | undefined) => (piece?.type === "word" ? piece.value.toUpperCase() : "");

/** Palabras que van entre `CREATE` y el tipo de objeto y no lo cambian. */
const CREATE_MODIFIERS = new Set([
  "OR", "REPLACE", "UNIQUE", "TEMP", "TEMPORARY", "UNLOGGED", "GLOBAL", "LOCAL", "RECURSIVE", "CONSTRAINT",
]);

function kindOf(pieces: Piece[]): string {
  const code = pieces.filter((p) => p.type !== "comment");
  const first = upper(code[0]);
  if (first === "WITH") {
    // El verbo de una consulta con CTE es el primero que aparece fuera de paréntesis.
    let depth = 0;
    for (let i = 1; i < code.length; i += 1) {
      const p = code[i];
      if (p.type === "punct" && p.value === "(") depth += 1;
      else if (p.type === "punct" && p.value === ")") depth -= 1;
      else if (depth === 0 && ["SELECT", "INSERT", "UPDATE", "DELETE", "MERGE"].includes(upper(p))) {
        return upper(p);
      }
    }
    return "WITH";
  }
  if (["CREATE", "ALTER", "DROP"].includes(first)) {
    let i = 1;
    while (CREATE_MODIFIERS.has(upper(code[i]))) i += 1;
    let object = upper(code[i]);
    if (["MATERIALIZED", "FOREIGN", "EVENT", "TEXT"].includes(object) && upper(code[i + 1]) !== "") {
      object += ` ${upper(code[i + 1])}`;
    }
    return object ? `${first} ${object}` : first;
  }
  return first || "?";
}

/** Una línea del código, sin saltos y recortada: lo que se lee en la lista cuando no hay título. */
function firstLine(text: string, from: number): string {
  const lines = text.slice(from).split(/\r?\n/);
  const line = lines.find((l) => l.trim() !== "") ?? "";
  const flat = line.replace(/\s+/g, " ").trim();
  return flat.length > 80 ? `${flat.slice(0, 79)}…` : flat;
}

function commentTitle(comment: Piece): string {
  const value = comment.value;
  const body = value.startsWith("--") ? value.slice(2) : value.replace(/^\/\*|\*\/$/g, "");
  const flat = body.replace(/\s+/g, " ").trim();
  return flat.length > 80 ? `${flat.slice(0, 79)}…` : flat;
}

export function outline(text: string): OutlineItem[] {
  const pieces = scan(text);
  const items: OutlineItem[] = [];

  let current: Piece[] = [];
  let comments: Piece[] = [];
  let atomic = false;
  let cases = 0;

  const lineOf = (index: number) => {
    let line = 1;
    for (let i = 0; i < index; i += 1) if (text[i] === "\n") line += 1;
    return line;
  };

  const close = (end: number) => {
    const code = current.filter((p) => p.type !== "comment");
    if (code.length > 0) {
      const from = code[0].start;
      // El comentario de justo arriba (una sola línea de por medio) es el título.
      const above = comments[comments.length - 1];
      const adjacent =
        above !== undefined && !/\n[ \t]*\r?\n/.test(text.slice(above.end, from));
      items.push({
        from,
        to: end,
        line: lineOf(from),
        kind: kindOf(current),
        title: adjacent ? commentTitle(above) || firstLine(text, from) : firstLine(text, from),
      });
    }
    current = [];
    comments = [];
    atomic = false;
    cases = 0;
  };

  for (let i = 0; i < pieces.length; i += 1) {
    const piece = pieces[i];
    if (piece.type === "comment") {
      if (current.every((p) => p.type === "comment")) comments.push(piece);
      current.push(piece);
      continue;
    }
    current.push(piece);

    if (piece.type === "word") {
      const word = piece.value.toUpperCase();
      if (word === "BEGIN" && upper(pieces[i + 1]) === "ATOMIC") atomic = true;
      else if (atomic && word === "CASE") cases += 1;
      else if (atomic && word === "END") {
        if (cases > 0) cases -= 1;
        else atomic = false;
      }
    } else if (piece.type === "punct" && piece.value === ";" && !atomic) {
      close(piece.end);
    }
  }
  const rest = current.filter((p) => p.type !== "comment");
  if (rest.length > 0) close(rest[rest.length - 1].end);
  return items;
}

/** Hasta el final de la sentencia del cursor; el cursor entre dos sentencias cuenta para la anterior. */
export function rangeUntil(text: string, cursor: number): string {
  const items = outline(text);
  const containing = items.find((item) => cursor >= item.from && cursor <= item.to);
  const target =
    containing ?? [...items].reverse().find((item) => item.to < cursor);
  return target ? text.slice(0, target.to).trim() : "";
}

/** Desde el comienzo de la sentencia del cursor; entre dos sentencias cuenta la que viene. */
export function rangeFrom(text: string, cursor: number): string {
  const items = outline(text);
  const containing = items.find((item) => cursor >= item.from && cursor <= item.to);
  const target = containing ?? items.find((item) => item.from > cursor);
  return target ? text.slice(target.from).trim() : "";
}
