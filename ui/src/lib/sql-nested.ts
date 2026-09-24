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
import { Tree, TreeFragment, type ChangedRange } from "@lezer/common";
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

/** Un documento más grande que esto es un volcado, no una consulta; se deja como está. */
const MAX_DOC = 400_000;

/** Un cambio de texto, en las coordenadas que entrega `ChangeSet.iterChangedRanges`. */
interface Change {
  fromA: number;
  toA: number;
  fromB: number;
  toB: number;
}

/**
 * Lleva los cambios de un `docChanged` a coordenadas del cuerpo, para poder reparsearlo en forma
 * incremental con `TreeFragment.applyChanges` en vez de tirar el árbol entero por una tecla.
 *
 * La garantía no es «todo cambio que toca el `$tag$` se detecta como cruce»: reemplazar el
 * delimitador de apertura entero por otro igual, sin rozar un solo carácter del cuerpo, entra por la
 * rama «antes» como cualquier edición ajena, y `null` sale recién del chequeo de largo del final —no
 * de una detección de cruce—. Lo que de verdad sostiene el resultado son dos cosas juntas: que quien
 * llama ya emparejó `old` y `next` por `mapPos(old.from, -1) === next.from` con la misma etiqueta
 * (ver `pairBodies`), y ese chequeo de largo, que atrapa lo que la posición sola no ve —un cambio que
 * cae entero «adentro» según sus coordenadas pero escribió un delimitador nuevo y cortó el cuerpo ahí
 * mismo, antes de donde el cambio en sí terminaba—. Cuando el cambio sí se sale de `[old.from,
 * old.to]` por un solo extremo, sin quedar enteramente afuera, se devuelve `null` directo, sin
 * llegar al chequeo de largo.
 *
 * Devuelve `[]`, distinto de `null`, cuando ningún cambio tocó el cuerpo: no hay nada que reaplicar y
 * el árbol de antes sigue valiendo tal cual.
 */
export function localChanges(
  changes: readonly Change[],
  old: DollarBlock,
  next: DollarBlock,
): ChangedRange[] | null {
  const local: ChangedRange[] = [];
  let delta = 0;

  for (const change of changes) {
    const { fromA, toA, fromB, toB } = change;

    // Adentro, incluidos los dos bordes: un cambio de largo cero justo en `old.from` es typear al
    // principio del cuerpo, y uno en `old.to` es typear justo antes del `$tag$` de cierre — los dos
    // son parte del cuerpo y no del delimitador, que no ocupa ningún carácter de `[from, to)`.
    if (fromA >= old.from && toA <= old.to) {
      local.push({
        fromA: fromA - old.from,
        toA: toA - old.from,
        fromB: fromB - next.from,
        toB: toB - next.from,
      });
      delta += toB - fromB - (toA - fromA);
      continue;
    }

    // Enteramente antes o enteramente después: no toca el cuerpo, solo corre sus posiciones —lo que
    // ya hizo `mapPos` para encontrar `next`.
    if (toA <= old.from || fromA >= old.to) continue;

    // Lo que queda cruza un borde del cuerpo.
    return null;
  }

  // El largo nuevo tiene que explicarse enteramente por lo que cambió adentro. Si no, el cuerpo que
  // `next` reporta no es el viejo editado in situ — por ejemplo, un `$$` escrito adentro corta el
  // cuerpo ahí mismo, antes de donde el cambio en sí mismo terminaba.
  if (next.to - next.from !== old.to - old.from + delta) return null;

  return local;
}

interface Body extends DollarBlock {
  /** `null` = sin parsear todavía, o desactualizado por un cambio que cruzó un borde. */
  tree: Tree | null;
  /** Lo reusable del parseo anterior, para reparsear incremental en vez de completo. */
  fragments: readonly TreeFragment[];
}

/**
 * Empareja cada cuerpo nuevo con el viejo que podría ser el mismo, para decidir en `localChanges`
 * si su árbol vale la pena reusarse. Puro y separado de `ViewUpdate` para poder probarlo sin armar
 * un `ChangeSet` real: `mapOldFrom` es justo `(from) => update.changes.mapPos(from, -1)`.
 *
 * El emparejamiento es solo un candidato, no una prueba: si dos cuerpos viejos, por la razón que
 * sea, mapean al mismo punto con la misma etiqueta, se prueba uno solo (el primero de `oldBodies`) y
 * queda en manos de `localChanges` rechazarlo si no calza —el chequeo de largo es la red que evita
 * reusar el árbol de un cuerpo que en realidad es otro.
 */
export function pairBodies<T extends DollarBlock>(
  oldBodies: readonly T[],
  next: readonly DollarBlock[],
  mapOldFrom: (from: number) => number,
): (T | null)[] {
  const mapped = oldBodies.map((old) => ({ old, from: mapOldFrom(old.from) }));
  return next.map((block) => {
    const found = mapped.find((m) => m.old.tag === block.tag && m.from === block.from);
    return found?.old ?? null;
  });
}

/**
 * El árbol de un cuerpo, parseado hasta `localTo` y ni un carácter más.
 *
 * Un cuerpo de seiscientas mil columnas se parsea entero en un cuarto de segundo, y nadie mira más
 * que la pantalla: `startParse` + `stopAt(localTo)` acota el costo a lo que se va a pintar. El árbol
 * que resulta es parcial (`TreeFragment.addTree(…, true)` le pone `openEnd`), y si después hace
 * falta ver más allá de donde se paró —scroll adentro del mismo cuerpo enorme, sin que el documento
 * haya cambiado, así que `update()` no tocó `bodies`— la llamada siguiente extiende desde ahí en vez
 * de arrancar de cero: los fragmentos ya cubren el tramo que se había parseado. El texto del cuerpo
 * solo se lee del documento (`sliceString`, que para un cuerpo grande no es gratis) cuando de verdad
 * hace falta parsear algo más; con el árbol ya cubriendo `localTo`, ni se toca.
 */
function parsed(view: EditorView, body: Body, localTo: number): Tree {
  if (body.tree && body.tree.length >= localTo) return body.tree;

  const text = view.state.doc.sliceString(body.from, body.to);
  const parse = PostgreSQL.language.parser.startParse(text, body.fragments);
  parse.stopAt(localTo);
  let tree = parse.advance();
  while (tree === null) tree = parse.advance();

  body.tree = tree;
  body.fragments = TreeFragment.addTree(tree, body.fragments, true);
  return tree;
}

function decorate(view: EditorView, bodies: readonly Body[]): DecorationSet {
  const { from: viewFrom, to: viewTo } = view.viewport;
  const ranges: Range<Decoration>[] = [];

  for (const body of bodies) {
    if (body.to < viewFrom || body.from > viewTo) continue;
    if (body.to === body.from) continue;

    // El cuerpo entero: adentro de la cadena que dibuja `lang-sql`, lo que ningún token pinte
    // seguiría saliendo del color de las cadenas, y un nombre de variable no es una cadena.
    ranges.push(BODY.range(body.from, body.to));

    const length = body.to - body.from;
    const localFrom = Math.max(0, Math.min(length, viewFrom - body.from));
    const localTo = Math.max(0, Math.min(length, viewTo - body.from));
    const tree = parsed(view, body, localTo);
    highlightTree(
      tree,
      sqlInlineHighlighter,
      (from, to, classes) => {
        const style = sqlInlineStyle(classes);
        if (!style) return;
        ranges.push(
          Decoration.mark({ attributes: { style } }).range(body.from + from, body.from + to),
        );
      },
      localFrom,
      localTo,
    );
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
 * No hay techo por tamaño de cuerpo: una función de mil líneas se coloreaba entera del verde de
 * cadena que pinta `lang-sql` en cuanto pasaba el límite de antes. Tres cosas lo permiten. Reparsear
 * es incremental (`localChanges` lleva cada cambio del documento a coordenadas del cuerpo y
 * `TreeFragment.applyChanges` reusa el árbol viejo fuera de ese tramo), así que lo que se paga por
 * tecla es el tamaño de lo que cambió y no el del cuerpo entero. El primer parseo de un cuerpo —o
 * cualquiera forzado de cero— está acotado a lo visible (`parsed`, con `stopAt`): sin eso, pegar una
 * función de seiscientas mil columnas se siente, un cuarto de segundo de bloqueo por parsearla
 * entera de una sola vez aunque solo se vean cuarenta líneas. Y se resalta solo lo que está a la
 * vista (`highlightTree` recibe el viewport recortado a coordenadas del cuerpo): desplazarse no
 * reparsea nada salvo que el cuerpo no llegue todavía hasta ahí, y un cuerpo que todavía no entró a
 * la vista no se toca.
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
      bodies: Body[];

      constructor(view: EditorView) {
        if (view.state.doc.length > MAX_DOC) {
          this.bodies = [];
          this.decorations = Decoration.none;
          return;
        }
        this.bodies = dollarBlocks(view.state.doc.toString()).map((block) => ({
          ...block,
          tree: null,
          fragments: [],
        }));
        this.decorations = decorate(view, this.bodies);
      }

      update(update: ViewUpdate) {
        if (update.docChanged) {
          if (update.state.doc.length > MAX_DOC) {
            this.bodies = [];
            this.decorations = Decoration.none;
            return;
          }

          const changes: Change[] = [];
          update.changes.iterChangedRanges((fromA, toA, fromB, toB) => {
            changes.push({ fromA, toA, fromB, toB });
          });

          const next = dollarBlocks(update.state.doc.toString());
          // `mapPos` traduce posiciones del documento viejo al nuevo. `-1`: lo que se inserta justo
          // en el primer carácter del cuerpo es del cuerpo y no del `$tag$` de apertura que lo
          // precede —con `+1` la posición mapeada saltaría detrás de lo insertado y typear ahí nunca
          // encontraría su par de antes—.
          const pairs = pairBodies(this.bodies, next, (from) => update.changes.mapPos(from, -1));
          this.bodies = next.map((block, i) => {
            const old = pairs[i];
            if (!old) return { ...block, tree: null, fragments: [] };

            const local = localChanges(changes, old, block);
            if (local === null) return { ...block, tree: null, fragments: [] };
            if (local.length === 0) return { ...block, tree: old.tree, fragments: old.fragments };

            return {
              ...block,
              tree: null,
              fragments: TreeFragment.applyChanges(old.fragments, local),
            };
          });
        }

        if (update.docChanged || update.viewportChanged) {
          this.decorations = decorate(update.view, this.bodies);
        }
      }
    },
    { decorations: (plugin) => plugin.decorations },
  ),
);
