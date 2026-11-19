import type { EditorState, TransactionSpec } from "@codemirror/state";
import type { SchemaRelation } from "../ipc";
import { expandStar } from "../sql-star";

/**
 * La edición de «expandir el asterisco» como un solo cambio: un `Ctrl+Z` la deshace entera. `null`
 * cuando no hay nada que expandir con certeza (ver `sql-star.ts`).
 */
export function starTransaction(
  state: EditorState,
  relations: SchemaRelation[],
): TransactionSpec | null {
  const expansion = expandStar(state.doc.toString(), state.selection.main.head, relations);
  if (!expansion) return null;
  return {
    changes: { from: expansion.from, to: expansion.to, insert: expansion.insert },
    selection: { anchor: expansion.from + expansion.insert.length },
    userEvent: "input.complete",
    scrollIntoView: true,
  };
}
