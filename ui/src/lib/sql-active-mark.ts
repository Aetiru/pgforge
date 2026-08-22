/**
 * El fondo tenue sobre la sentencia que `Ctrl+Enter` va a ejecutar, mientras el cursor esté adentro.
 *
 * Calcado de `sql-error-mark.ts`: mismo motivo para que sea un `StateField` y no un `DecorationSet`
 * fijo —tiene que mapearse con cada cambio del documento, o una edición corrida deja la marca
 * apuntando a caracteres que ya no son los mismos— y mismo `toEditorIndex` para pasar de caracteres
 * del núcleo a unidades UTF-16 del editor.
 *
 * Quién decide cuál es la sentencia activa no vive acá: lo resuelve `sql::split::at_cursor` del
 * lado de Rust (vía `statementAtCursor`), y `QueryPanel` es quien pregunta y pasa el rango ya
 * resuelto. Reescribir esa regla en TypeScript sería el mismo error que ya se evitó con
 * `statementAtCursor` para «Ejecutar».
 */

import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";
import { toEditorIndex } from "./sql-error-mark";

/** La sentencia activa, en caracteres del núcleo (no UTF-16, no unidades del editor). */
export interface ActiveRange {
  at: number;
  length: number;
}

/** El fondo sobre el rango, o nada si no hay sentencia activa que marcar. */
export function activeMarkOf(text: string, range: ActiveRange | null): DecorationSet {
  if (!range || range.length <= 0) return Decoration.none;

  const from = toEditorIndex(text, range.at);
  const to = toEditorIndex(text, range.at + range.length);
  if (from >= text.length || to <= from) return Decoration.none;

  return Decoration.set([
    Decoration.mark({ class: "cm-active-statement" }).range(from, Math.min(to, text.length)),
  ]);
}

/** Repone el fondo. `Decoration.none` lo saca. */
export const setActiveMark = StateEffect.define<DecorationSet>();

/** El fondo vigente, mapeado con cada cambio del documento — mismo motivo que `errorMarkField`. */
export const activeMarkField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    for (const effect of tr.effects) if (effect.is(setActiveMark)) return effect.value;
    return marks.map(tr.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});
