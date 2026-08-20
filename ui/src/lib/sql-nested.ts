/**
 * El SQL que vive adentro de una cadena delimitada con `$$`.
 *
 * Para el tokenizador de `lang-sql`, el cuerpo de una función es una cadena y nada más: cien líneas
 * de plpgsql salen todas del mismo verde, sin poder distinguir una palabra clave de un nombre de
 * columna. Acá se ubican esos cuerpos para volver a colorearlos como lo que son.
 *
 * La búsqueda es puramente textual y no usa el árbol de sintaxis a propósito: mientras se escribe,
 * el árbol tiene nodos de error justo donde está el cursor —lo mismo que ya llevó a leer el `FROM`
 * a mano en [`sql-complete`]—, y una función a medio escribir es el caso normal, no la excepción.
 */

import { PostgreSQL } from "@codemirror/lang-sql";
import { Prec, type Range } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { highlightTree } from "@lezer/highlight";
import { sqlInlineHighlighter, sqlInlineStyle } from "./sql-highlight";

export interface DollarBlock {
  /** Primer carácter del cuerpo, ya pasado el `$tag$` de apertura. */
  from: number;
  /** Primer carácter del `$tag$` de cierre, o el final del texto si quedó sin cerrar. */
  to: number;
  /** La etiqueta sin los `$`; vacía en el caso corriente, `$$`. */
  tag: string;
}

/** El `$tag$` que abre o cierra un cuerpo, con la etiqueta vacía como caso válido. */
const DELIMITER = /^\$([\p{L}_][\p{L}\p{N}_]*)?\$/u;

/**
 * Los cuerpos delimitados con `$…$` que hay en el texto.
 *
 * Se saltean las cadenas comunes, los identificadores entre comillas y los comentarios porque un
 * `$$` escrito ahí adentro es texto y no abre nada; sin eso, un comentario que menciona `$$` dejaría
 * coloreado como cuerpo de función todo lo que sigue.
 */
export function dollarBlocks(text: string): DollarBlock[] {
  const out: DollarBlock[] = [];
  let i = 0;

  while (i < text.length) {
    const char = text[i];

    if (char === "'" || char === '"') {
      // Adentro de una cadena o de un identificador citado, el delimitador repetido es un escape:
      // `''` no cierra nada, sigue siendo la misma cadena.
      i += 1;
      while (i < text.length) {
        if (text[i] === char) {
          if (text[i + 1] === char) i += 2;
          else break;
        } else {
          i += 1;
        }
      }
      i += 1;
      continue;
    }

    if (char === "-" && text[i + 1] === "-") {
      const end = text.indexOf("\n", i);
      i = end === -1 ? text.length : end + 1;
      continue;
    }

    if (char === "/" && text[i + 1] === "*") {
      // Los comentarios de bloque de PostgreSQL anidan, al revés que los de C.
      let depth = 1;
      i += 2;
      while (i < text.length && depth > 0) {
        if (text[i] === "/" && text[i + 1] === "*") {
          depth += 1;
          i += 2;
        } else if (text[i] === "*" && text[i + 1] === "/") {
          depth -= 1;
          i += 2;
        } else {
          i += 1;
        }
      }
      continue;
    }

    if (char === "$") {
      const match = DELIMITER.exec(text.slice(i));
      if (match) {
        const delimiter = match[0];
        const from = i + delimiter.length;
        const close = text.indexOf(delimiter, from);
        // Sin cierre, el cuerpo es todo lo que sigue: es exactamente lo que se está viendo mientras
        // se escribe la función, y es cuando más falta hace el color.
        const to = close === -1 ? text.length : close;
        out.push({ from, to, tag: match[1] ?? "" });
        i = close === -1 ? text.length : close + delimiter.length;
        continue;
      }
    }

    i += 1;
  }

  return out;
}

/**
 * Más allá de esto no se recolorea nada: un cuerpo así no se lee, y reparsearlo en cada tecla es lo
 * único de todo esto que se puede sentir al escribir.
 */
const MAX_BODY = 40_000;

/** Un documento más grande que esto es un volcado, no una consulta; se deja como está. */
const MAX_DOC = 400_000;

function decorate(view: EditorView): DecorationSet {
  if (view.state.doc.length > MAX_DOC) return Decoration.none;

  const text = view.state.doc.toString();
  const { from: viewFrom, to: viewTo } = view.viewport;
  const ranges: Range<Decoration>[] = [];

  for (const block of dollarBlocks(text)) {
    if (block.to < viewFrom || block.from > viewTo) continue;
    const length = block.to - block.from;
    if (length === 0 || length > MAX_BODY) continue;

    // El cuerpo entero: adentro de la cadena que dibuja `lang-sql`, lo que ningún token pinte
    // seguiría saliendo del color de las cadenas, y un nombre de variable no es una cadena.
    ranges.push(BODY.range(block.from, block.to));

    const tree = PostgreSQL.language.parser.parse(text.slice(block.from, block.to));
    highlightTree(tree, sqlInlineHighlighter, (from, to, classes) => {
      const style = sqlInlineStyle(classes);
      if (!style) return;
      ranges.push(
        Decoration.mark({ attributes: { style } }).range(block.from + from, block.from + to),
      );
    });
  }

  // Ordena el conjunto en vez de armarlo con un `RangeSetBuilder`: la decoración del cuerpo entero
  // se superpone con la de cada token de adentro, y ahí el orden en que se agregan no alcanza.
  return Decoration.set(ranges, true);
}

/** Devuelve al color del texto común lo que el resaltado de adentro no pinte. */
const BODY = Decoration.mark({ attributes: { style: "color: var(--cm-text)" } });

/**
 * Colorea como SQL lo que hay adentro de cada `$$ … $$`.
 *
 * Se recalcula sobre lo que está a la vista y no sobre el documento entero: un archivo con veinte
 * funciones se reparsearía completo en cada tecla para dibujar las tres líneas que se ven.
 *
 * Va en `Prec.highest` y eso **no es un detalle de orden**: con marcas superpuestas, CodeMirror
 * dibuja adentro las de mayor precedencia, y el color lo decide el `span` de más adentro. Con la
 * precedencia por omisión, el de la cadena que `lang-sql` pinta sobre todo el cuerpo quedaba como el
 * último hijo y ganaba siempre —el cuerpo entero de un verde parejo, que es el problema que esto
 * viene a resolver—.
 */
export const sqlNesting = Prec.highest(
  ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;

      constructor(view: EditorView) {
        this.decorations = decorate(view);
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) {
          this.decorations = decorate(update.view);
        }
      }
    },
    { decorations: (plugin) => plugin.decorations },
  ),
);
