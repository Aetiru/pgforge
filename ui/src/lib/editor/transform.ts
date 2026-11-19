import type { EditorState, TransactionSpec } from "@codemirror/state";
import { outline } from "../sql-outline";
import { transformText, type TextTransform } from "../sql-text-tools";

/**
 * Aplica una transformación de texto: a la selección si hay, y si no a la sentencia donde está el
 * cursor (o al documento entero si el cursor no cae en ninguna). Sin selección, tocar solo la
 * sentencia es lo que se espera de «a mayúsculas» en un script de veinte: nadie quiere que le suban
 * todo.
 *
 * `null` si el resultado es igual al original: no hay nada que deshacer ni que avisar.
 */
export function transformTransaction(
  state: EditorState,
  kind: TextTransform,
): TransactionSpec | null {
  const { from, to, head } = state.selection.main;
  const text = state.doc.toString();

  let start = from;
  let end = to;
  if (from === to) {
    const statement = outline(text).find((item) => head >= item.from && head <= item.to);
    start = statement?.from ?? 0;
    end = statement?.to ?? text.length;
  }

  const original = text.slice(start, end);
  const changed = transformText(kind, original);
  if (changed === original) return null;
  return {
    changes: { from: start, to: end, insert: changed },
    selection: { anchor: start, head: start + changed.length },
    userEvent: "input",
    scrollIntoView: true,
  };
}
