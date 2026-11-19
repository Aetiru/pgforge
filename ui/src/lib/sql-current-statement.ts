/**
 * La sentencia que se ejecutaría con `Ctrl+Enter`, marcada en el borde del editor.
 *
 * Con varias consultas escritas, quién ejecuta es la del cursor, y hasta apretar no se sabía cuál.
 * Es un campo de estado y no un conjunto fijo por la misma razón que `sql-error-mark`: se mapea con
 * cada cambio del documento, o la marca quedaría clavada en un desplazamiento que el próximo
 * borrado deja afuera del texto.
 */

import { StateEffect, StateField, type Text } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";

/** Una marca por línea, de `from` a `to` (índices del editor). */
export function linesOf(doc: Text, from: number, to: number): DecorationSet {
  if (from >= to || from < 0 || to > doc.length) return Decoration.none;
  const marks = [];
  const last = doc.lineAt(to).number;
  for (let number = doc.lineAt(from).number; number <= last; number += 1) {
    marks.push(Decoration.line({ class: "cm-current-statement" }).range(doc.line(number).from));
  }
  return Decoration.set(marks);
}

export const setCurrentStatement = StateEffect.define<DecorationSet>();

export const currentStatementField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    for (const effect of tr.effects) if (effect.is(setCurrentStatement)) return effect.value;
    return marks.map(tr.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});
