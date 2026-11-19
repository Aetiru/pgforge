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
import type { EditorView, KeyBinding } from "@codemirror/view";
import type { Snippet } from "./ipc";
import { readWithTimeout, withSelection, type ShortcutEntry } from "./editor/snippet-env";
import {
  autoVariablesOf,
  fillAutomatic,
  fillVariables,
  variablesOf,
  type AutoName,
  type SnippetVariable,
} from "./snippet-vars";

/**
 * Pide los valores de las variables (`${nombre:valor}`) y llama a `done` con ellos; si se cancela,
 * `done` no se llama y no se inserta nada. Lo implementa quien tiene un lugar donde mostrar el
 * diálogo —`SqlEditor`—, para que este módulo siga siendo CodeMirror y texto, sin componentes.
 */
export type AskVariables = (
  snippet: { abbreviation: string; description: string },
  variables: SnippetVariable[],
  done: (values: Record<string, string>) => void,
) => void;

/**
 * Lo que la expansión necesita saber del lugar donde ocurre. Va aparte del editor para que este
 * módulo siga siendo CodeMirror y texto: quien lo arma (`SqlEditor`) conoce la pestaña.
 */
export interface ExpandEnv {
  /** Los automáticos conocidos de la pestaña (`${hoy}`, `${base}`…), sin selección ni portapapeles. */
  auto: () => Partial<Record<AutoName, string>>;
  /** Lee el portapapeles. Sin esto `${portapapeles}` queda como hueco común. */
  clipboard?: () => Promise<string>;
  /** Se llama cuando la abreviatura **se insertó de verdad** —no si se cancela el diálogo—. */
  used?: (id: string) => void;
}

/** Cuánto se espera al portapapeles antes de dejar el hueco sin llenar (ver `readWithTimeout`). */
const CLIPBOARD_WAIT_MS = 400;

/** Lo mínimo de una abreviatura que hace falta para expandirla; el `id` alimenta el contador de usos. */
export type Expandable = Pick<Snippet, "abbreviation" | "body" | "description"> & { id?: string };

/**
 * Inserta el cuerpo en `[from, to]`.
 *
 * El orden es siempre el mismo: primero los valores automáticos, después las variables que se
 * preguntan y por último los huecos de CodeMirror (`snippet()`). Los automáticos se llenan primero
 * porque `${seleccion}` puede traer SQL con `${` adentro y entra ya escapado de llaves, así que ni
 * las variables ni los huecos lo vuelven a leer.
 *
 * `selection` es lo seleccionado **al expandir** (atajo propio de la abreviatura, selector); con el
 * tabulador no hay ninguna, y `${seleccion}` queda como hueco donde escribir.
 */
export function insertSnippet(
  view: EditorView,
  item: Expandable,
  from: number,
  to: number,
  ask?: AskVariables,
  env?: ExpandEnv,
  selection?: string,
) {
  const base = withSelection(env?.auto() ?? {}, selection);
  const go = (context: Partial<Record<AutoName, string>>) =>
    expand(view, item, from, to, ask, env, fillAutomatic(item.body, context));

  if (!env?.clipboard || !autoVariablesOf(item.body).includes("portapapeles")) {
    go(base);
    return;
  }

  // Solo se toca el portapapeles si el cuerpo lo usa: leerlo puede pedirle permiso al sistema.
  const original = view.state.sliceDoc(from, to);
  void readWithTimeout(env.clipboard, CLIPBOARD_WAIT_MS).then((text) => {
    // Si mientras tanto se tecleó sobre ese lugar, insertar ahí corrompería lo escrito.
    if (view.state.doc.length < to || view.state.sliceDoc(from, to) !== original) return;
    go(text === null ? base : { ...base, portapapeles: text });
  });
}

function expand(
  view: EditorView,
  item: Expandable,
  from: number,
  to: number,
  ask: AskVariables | undefined,
  env: ExpandEnv | undefined,
  body: string,
) {
  const variables = variablesOf(body);
  const insert = (text: string) => {
    snippet(text)(view, null, from, to);
    if (item.id) env?.used?.(item.id);
  };
  if (variables.length === 0 || !ask) {
    // Sin donde preguntar, las variables valen su valor por omisión: mejor eso que dejar el
    // `${nombre:valor}` escrito a la vista.
    insert(fillVariables(body, {}));
    return;
  }
  ask(item, variables, (values) => {
    insert(fillVariables(body, values));
    view.focus();
  });
}

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
 * Devuelve `false` —y deja que lo atienda quien sigue: el salto entre los huecos de una expansión
 * abierta o, si no hay ninguno, el tabulador que indenta— en los cuatro casos en que no le toca: con
 * un hueco de otra expansión abierto, con algo seleccionado, sin palabra donde está el cursor, y con
 * una palabra que no es ninguna abreviatura. Es la misma disciplina que ya pedía el `Escape` de este
 * editor: el keymap va en `Prec.highest` y se come lo que no devuelva `false`.
 *
 * Lo primero no es una precaución teórica: recién expandido, el cursor queda **adentro** de un hueco
 * cuyo texto por omisión es una palabra (`${tabla}`), así que sin esa guarda un hueco que se llame
 * como una abreviatura volvería a expandirse en vez de saltar al siguiente.
 */
export function expandBinding(
  snippets: () => readonly Snippet[],
  ask?: AskVariables,
  env?: ExpandEnv,
): KeyBinding {
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
      insertSnippet(view, found, at.from, at.to, ask, env);
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
export function snippetCompletions(
  snippets: () => readonly Snippet[],
  ask?: AskVariables,
  env?: ExpandEnv,
) {
  return (context: CompletionContext): CompletionResult | null => {
    const word = context.matchBefore(/[\p{L}\p{N}_]+/u);
    if (!word || (word.from === word.to && !context.explicit)) return null;

    const options: Completion[] = snippets().map((item) =>
      ({
        ...snippetCompletion(item.body, {
          label: item.abbreviation,
          detail: item.description || preview(item.body),
          type: "keyword",
          // Por encima de las tablas y las palabras clave: quien escribió `sf` y lo definió como
          // abreviatura quiso eso y no una columna que empiece igual.
          boost: 50,
        }),
        // Elegirla de la lista pasa por el mismo camino que el tabulador, o las variables solo
        // se preguntarían por uno de los dos.
        apply: (view: EditorView, _completion: Completion, from: number, to: number) =>
          insertSnippet(view, item, from, to, ask, env),
      }) as Completion,
    );

    return { from: word.from, options, validFor: /^[\p{L}\p{N}_]*$/u };
  };
}

/**
 * Un atajo de teclado por abreviatura: la combinación expande **esa** abreviatura donde está el
 * cursor, sin escribir su nombre. Con algo seleccionado lo reemplaza y `${seleccion}` lo recibe, que
 * es para lo que sirve una abreviatura como «envolver».
 *
 * Quién gana ante un choque ya está decidido en `shortcutEntries`: acá solo se arma lo que quedó.
 * Devuelve `false` en un editor de solo lectura, para que la combinación siga su camino.
 */
export function snippetShortcutBindings(
  entries: readonly ShortcutEntry[],
  ask?: AskVariables,
  env?: ExpandEnv,
): KeyBinding[] {
  return entries.map(({ combo, snippet: item }) => ({
    key: combo,
    preventDefault: true,
    run: (view: EditorView) => {
      if (view.state.readOnly) return false;
      const { from, to, empty } = view.state.selection.main;
      closeCompletion(view);
      insertSnippet(view, item, from, to, ask, env, empty ? undefined : view.state.sliceDoc(from, to));
      return true;
    },
  }));
}
