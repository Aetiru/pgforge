/**
 * Transformaciones de texto del editor: pegar una columna como lista y los arreglos de siempre
 * (mayúsculas, unir líneas, citar…). Puro: el editor solo aplica el resultado a la selección.
 */

export type TextTransform =
  | "upper"
  | "lower"
  | "joinComma"
  | "quoteLines"
  | "unquote"
  | "inList"
  | "trim"
  | "sortLines"
  | "uniqueLines";

/** Con qué palabra empieza un texto que ya es SQL y no una lista de valores. */
const SQL_START =
  /^\s*(select|insert|update|delete|with|create|alter|drop|truncate|begin|commit|rollback|values|explain|set|grant|revoke|analyze|vacuum|copy)\b/i;

/** Un entero o decimal escrito como lo escribiría SQL. `00123` no: es un código, no un número. */
const NUMBER = /^-?(0|[1-9]\d*)(\.\d+)?$/;

const quote = (value: string) => `'${value.replace(/'/g, "''")}'`;

function literalList(items: string[]): string {
  const numeric = items.every((item) => NUMBER.test(item));
  return `(${items.map((item) => (numeric ? item : quote(item))).join(", ")})`;
}

/** Líneas no vacías, o las celdas de una fila si el texto es una sola línea con tabuladores. */
function cells(text: string): string[] | null {
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line !== "");
  if (lines.length === 1) {
    return lines[0].includes("\t")
      ? lines[0].split("\t").map((cell) => cell.trim()).filter((cell) => cell !== "")
      : [lines[0]];
  }
  // Varias líneas con tabuladores son una tabla de dos dimensiones: no hay una lista que sacar.
  if (lines.some((line) => line.includes("\t"))) return null;
  return lines;
}

/**
 * Convierte lo pegado —una columna de Excel, una por línea, o una fila separada por tabuladores—
 * en una lista lista para un `IN (…)`.
 *
 * Los números van sin comillas **solo si todos lo son**: una columna con `00123` y `456` es de
 * códigos, y sin las comillas el primero perdería los ceros. `null` cuando no corresponde tocar el
 * pegado: una sola línea (es texto normal) o algo que ya es SQL.
 */
export function pasteAsList(text: string): string | null {
  if (text.includes(";") || SQL_START.test(text) || /^\s*(--|\/\*)/.test(text)) return null;
  const items = cells(text);
  if (items === null || items.length < 2) return null;
  return literalList(items);
}

/** Lo que el servidor lee como código y no como dato: comentarios, cadenas, citados y `$$ … $$`. */
function protectedRanges(text: string): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    const next = text[i + 1];
    let end = -1;
    if (c === "-" && next === "-") {
      const nl = text.indexOf("\n", i);
      end = nl === -1 ? text.length : nl;
    } else if (c === "/" && next === "*") {
      const close = text.indexOf("*/", i + 2);
      end = close === -1 ? text.length : close + 2;
    } else if (c === "'" || c === '"') {
      let j = i + 1;
      while (j < text.length) {
        if (text[j] === c && text[j + 1] === c) j += 2;
        else if (text[j] === c) {
          j += 1;
          break;
        } else j += 1;
      }
      end = j;
    } else if (c === "$") {
      const tag = /^\$([A-Za-z_]\w*)?\$/.exec(text.slice(i, i + 64));
      if (tag) {
        const close = text.indexOf(tag[0], i + tag[0].length);
        end = close === -1 ? text.length : close + tag[0].length;
      }
    }
    if (end === -1) i += 1;
    else {
      ranges.push([i, end]);
      i = end;
    }
  }
  return ranges;
}

const SQL_WORDS =
  /\b(select|from|where|insert|update|delete|join|create|alter|drop|group|order|limit|values|having|union|begin|commit)\b/i;

/** ¿Es código SQL? Un apóstrofe de una frase («don't») no tiene que proteger el resto del texto. */
function looksLikeSql(text: string): boolean {
  return SQL_WORDS.test(text) || /--|\/\*|\$\$/.test(text);
}

/**
 * Mayúsculas o minúsculas **sin tocar los datos**. Sobre SQL no se cambian los literales
 * (`'Ana'` seguiría igual), los identificadores con comillas, los comentarios ni los cuerpos
 * `$$ … $$`: cambiarlos cambia lo que la consulta busca o cómo se llama el objeto. Sobre un texto
 * que no parece SQL se cambia todo, que es lo que se espera de «a mayúsculas».
 */
function changeCase(text: string, toUpper: boolean): string {
  const convert = (piece: string) => (toUpper ? piece.toUpperCase() : piece.toLowerCase());
  if (!looksLikeSql(text)) return convert(text);

  let result = "";
  let last = 0;
  for (const [from, to] of protectedRanges(text)) {
    result += convert(text.slice(last, from)) + text.slice(from, to);
    last = to;
  }
  return result + convert(text.slice(last));
}

export function transformText(kind: TextTransform, text: string): string {
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const lines = () => text.split(/\r?\n/);

  switch (kind) {
    case "upper":
      return changeCase(text, true);
    case "lower":
      return changeCase(text, false);

    case "joinComma":
      return lines()
        .map((line) => line.trim())
        .filter((line) => line !== "")
        .join(", ");

    case "quoteLines":
      // Las líneas vacías quedan vacías: se conserva la forma del texto.
      return lines()
        .map((line) => (line.trim() === "" ? line : quote(line.trim())))
        .join(eol);

    case "unquote":
      // Solo comillas simples: las dobles pueden ser lo que hace válido un identificador.
      return text.replace(/'((?:[^']|'')*)'/g, (_all, inner: string) => inner.replace(/''/g, "'"));

    case "inList": {
      let items = cells(text) ?? lines().map((line) => line.trim()).filter((line) => line !== "");
      // «a, b, c» en una sola línea: la lista ya está escrita, falta el formato.
      if (items.length === 1 && items[0].includes(",")) {
        items = items[0]
          .split(",")
          .map((item) => item.trim().replace(/^'((?:[^']|'')*)'$/, (_all, inner: string) => inner.replace(/''/g, "'")))
          .filter((item) => item !== "");
      }
      return items.length === 0 ? text : literalList(items);
    }

    case "trim":
      // Solo el final de cada línea: la sangría del SQL es contenido.
      return lines()
        .map((line) => line.trimEnd())
        .join(eol)
        .replace(/^(?:\s*\r?\n)+/, "")
        .replace(/(?:\r?\n\s*)+$/, "");

    case "sortLines":
      return lines()
        .filter((line) => line.trim() !== "")
        .sort((a, b) => a.localeCompare(b, "es", { numeric: true, sensitivity: "base" }))
        .join(eol);

    case "uniqueLines": {
      const seen = new Set<string>();
      return lines()
        .filter((line) => {
          if (line.trim() === "") return true;
          if (seen.has(line)) return false;
          seen.add(line);
          return true;
        })
        .join(eol);
    }
  }
}
