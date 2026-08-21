/**
 * Abreviaturas: `sf` y un tabulador en vez de escribir `SELECT * FROM ` cada vez.
 *
 * La expansión mira la palabra pegada al cursor, no el árbol de sintaxis: al escribir, el árbol
 * tiene nodos de error justo ahí —el mismo motivo que ya llevó a leer el `FROM` a mano en
 * [`sql-complete`] y a ubicar los `$$` por texto en [`sql-nested`]—.
 *
 * El texto se inserta con `snippet()` de CodeMirror, así que los `${}` del cuerpo son huecos por los
 * que se salta con el tabulador. Los campos **exigen llaves**, de modo que el `$1` de un parámetro
 * de PostgreSQL sigue siendo texto literal y no un hueco: es la razón por la que se pudo usar la
 * sintaxis de CodeMirror tal cual en vez de inventar otra.
 */

import {
  closeCompletion,
  hasNextSnippetField,
  snippet,
  snippetCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import type { KeyBinding } from "@codemirror/view";
import type { Snippet } from "./ipc";

/** Lo que puede formar una abreviatura: lo mismo que un identificador de SQL sin comillas. */
const WORD = /[\p{L}\p{N}_]/u;

export interface WordAt {
  from: number;
  to: number;
  word: string;
}

/**
 * La palabra que termina justo en `pos`, o `null` si ahí no termina ninguna.
 *
 * «Justo en» es lo que hace que esto sea predecible: con el cursor en medio de `select`, un
 * tabulador no tiene por qué expandir `sel`. Se pide el final de la palabra a propósito.
 */
export function wordBefore(text: string, pos: number): WordAt | null {
  let from = pos;
  while (from > 0 && WORD.test(text[from - 1])) from -= 1;
  if (from === pos) return null;
  return { from, to: pos, word: text.slice(from, pos) };
}

/**
 * La abreviatura que coincide con esa palabra, sin distinguir mayúsculas.
 *
 * Sin distinguirlas porque quien la definió como `sf` la escribe `SF` a mitad de una consulta en
 * mayúsculas sin pensarlo, y que ahí no funcione se lee como que la función está rota.
 */
export function findSnippet(snippets: readonly Snippet[], word: string): Snippet | null {
  const wanted = word.toLowerCase();
  return snippets.find((s) => s.abbreviation.toLowerCase() === wanted) ?? null;
}

/** El texto sin los huecos, para mostrarlo en una lista donde `${tabla}` es ruido. */
export function preview(body: string): string {
  return body
    .replace(/\\([{}])/g, "$1")
    .replace(/[$#]\{([^}]*)\}/g, (_, inner: string) => {
      // `${1:por omisión}` muestra lo de después de los dos puntos; `${1}` y `${}` no muestran nada.
      const text = inner.includes(":") ? inner.slice(inner.indexOf(":") + 1) : inner;
      return /^\d*$/.test(text) ? "" : text;
    })
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * El atajo del tabulador.
 *
 * Devuelve `false` —y deja que lo atienda quien sigue— en los tres casos en que no le toca: con un
 * hueco de otra expansión abierto, sin palabra donde está el cursor, y con una palabra que no es
 * ninguna abreviatura. Es la misma disciplina que ya pedía el `Escape` de este editor: el keymap va
 * en `Prec.highest` y se come lo que no devuelva `false`.
 *
 * Lo primero no es una precaución teórica: recién expandido, el cursor queda **adentro** de un hueco
 * cuyo texto por omisión es una palabra (`${tabla}`), así que sin esa guarda un hueco que se llame
 * como una abreviatura volvería a expandirse en vez de saltar al siguiente.
 */
export function expandBinding(snippets: () => readonly Snippet[]): KeyBinding {
  return {
    key: "Tab",
    run: (view) => {
      if (hasNextSnippetField(view.state)) return false;

      const { from: cursor, empty } = view.state.selection.main;
      if (!empty) return false;

      const at = wordBefore(view.state.doc.toString(), cursor);
      if (!at) return false;

      const found = findSnippet(snippets(), at.word);
      if (!found) return false;

      // La lista de sugerencias puede haber quedado abierta sobre la abreviatura: expandir por
      // debajo dejaría el popup flotando contra un texto que ya no existe.
      closeCompletion(view);
      snippet(found.body)(view, null, at.from, at.to);
      return true;
    },
  };
}

/**
 * Las abreviaturas como fuente de autocompletado, sumada a las que ya hay.
 *
 * Que además aparezcan en la lista es lo que las hace descubribles: con solo el tabulador hay que
 * acordarse de cuáles existen, y una función que se olvida no se usa.
 */
export function snippetCompletions(snippets: () => readonly Snippet[]) {
  return (context: CompletionContext): CompletionResult | null => {
    const word = context.matchBefore(/[\p{L}\p{N}_]+/u);
    if (!word || (word.from === word.to && !context.explicit)) return null;

    const options: Completion[] = snippets().map((item) =>
      snippetCompletion(item.body, {
        label: item.abbreviation,
        detail: item.description || preview(item.body),
        type: "keyword",
        // Por encima de las tablas y las palabras clave: quien escribió `sf` y lo definió como
        // abreviatura quiso eso y no una columna que empiece igual.
        boost: 50,
      }),
    );

    return { from: word.from, options, validFor: /^[\p{L}\p{N}_]*$/u };
  };
}
