import { foldService } from "@codemirror/language";
import type { Text } from "@codemirror/state";
import { outline, type OutlineItem } from "../sql-outline";

/** Una sentencia de menos líneas que esto no gana nada plegándose. */
export const MIN_FOLD_LINES = 3;

/**
 * El tramo que se pliega si en la línea `[lineFrom, lineTo]` empieza una sentencia: desde el final de
 * esa primera línea hasta el final de la sentencia, de modo que la cabecera (`SELECT …`, `CREATE
 * FUNCTION …`) queda a la vista. Puro, para probarlo sin editor.
 */
export function statementFold(
  items: OutlineItem[],
  text: string,
  lineFrom: number,
  lineTo: number,
): { from: number; to: number } | null {
  const item = items.find((candidate) => candidate.from >= lineFrom && candidate.from <= lineTo);
  if (!item || item.to <= lineTo) return null;

  let lines = 1;
  for (let at = item.from; at < item.to; at += 1) if (text.charCodeAt(at) === 10) lines += 1;
  return lines >= MIN_FOLD_LINES ? { from: lineTo, to: item.to } : null;
}

// `foldService` se llama una vez por línea visible: partir el script en cada llamada sería
// cuadrático. Se recuerda por documento —CodeMirror usa un `Text` nuevo con cada cambio—.
const cache = new WeakMap<Text, { text: string; items: OutlineItem[] }>();

function analysis(doc: Text) {
  let hit = cache.get(doc);
  if (!hit) {
    const text = doc.toString();
    hit = { text, items: outline(text) };
    cache.set(doc, hit);
  }
  return hit;
}

/** Plegar sentencia por sentencia con el margen de plegado que ya trae `basicSetup`. */
export const statementFolding = foldService.of((state, lineFrom, lineTo) => {
  const { text, items } = analysis(state.doc);
  return statementFold(items, text, lineFrom, lineTo);
});
