/**
 * El subrayado del error de la última ejecución, dentro del editor.
 *
 * PostgreSQL contesta con la posición exacta donde falló, y marcarla es más útil que repetir el
 * mensaje arriba del editor. Lo que no es obvio es **dónde vive** esa marca: no puede ser un
 * `DecorationSet` fijo, tiene que ser un campo de estado que se mapee con cada cambio del documento.
 *
 * Un conjunto instalado una sola vez queda escrito contra el documento de ese momento. El error se
 * marca en el 62, el usuario borra lo que había, y el subrayado sigue apuntando al 62 de un texto
 * que ya no llega hasta ahí. CodeMirror no lo descubre al borrar: lo descubre en la primera
 * transacción que venga después —un clic alcanza, o la medición del ancho— y desde entonces tira
 * `RangeError: Position 62 is out of range for changeset of length 0` en **cada** una, porque la
 * marca vencida sigue ahí. Como campo, se mapea: la marca sigue a su palabra mientras la palabra
 * exista y se va con ella cuando se la borra, que es además lo que uno espera al corregir el error.
 */

import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import type { ErrorMark } from "./query.svelte";

/**
 * De un desplazamiento en caracteres al índice que usa el editor.
 *
 * El núcleo cuenta caracteres y CodeMirror cuenta unidades UTF-16, así que un emoji o un acento
 * compuesto en el SQL corren la marca si se pasa el número tal cual. Se exporta porque
 * `sql-active-mark.ts` necesita exactamente la misma conversión para la sentencia activa.
 */
export function toEditorIndex(text: string, chars: number): number {
  return [...text].slice(0, chars).join("").length;
}

/** Subraya el error abarcando la palabra entera, que es más fácil de ver que un solo carácter. */
export function markOf(text: string, mark: ErrorMark | null): DecorationSet {
  const from = mark ? toEditorIndex(text, mark.at) : 0;
  if (!mark || from >= text.length) return Decoration.none;

  let to = from + 1;
  while (to < text.length && /[\w$."]/.test(text[to])) to += 1;

  return Decoration.set([
    Decoration.mark({ class: "cm-error-mark", attributes: { title: mark.message } }).range(from, to),
  ]);
}

/** Repone el subrayado. `Decoration.none` lo saca. */
export const setErrorMark = StateEffect.define<DecorationSet>();

/**
 * El subrayado vigente, mapeado con cada cambio del documento.
 *
 * El `map` es todo el asunto: sin él, cualquier edición que acorte el texto deja la marca apuntando
 * afuera y rompe todas las transacciones que sigan.
 */
export const errorMarkField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    for (const effect of tr.effects) if (effect.is(setErrorMark)) return effect.value;
    return marks.map(tr.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});
