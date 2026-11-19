import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { pasteAsList } from "../sql-text-tools";

/**
 * Cuánto dura la intención de «pegar como lista» desde que se aprieta la tecla. El navegador manda
 * el evento `paste` enseguida; si no llega —el portapapeles estaba vacío—, la bandera no puede
 * quedar armada para el próximo `Ctrl+V` común.
 */
export const PASTE_INTENT_MS = 1500;

/** Bandera de un solo uso: se arma con el atajo y la consume el evento `paste` que sigue. */
export class PasteIntent {
  private at = 0;

  arm(now: number) {
    this.at = now;
  }

  /** `true` una sola vez por cada vez que se armó, y solo si no pasó mucho. */
  consume(now: number): boolean {
    const armed = this.at > 0 && now - this.at <= PASTE_INTENT_MS;
    this.at = 0;
    return armed;
  }
}

export function isPasteAsListKey(event: {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}): boolean {
  return (event.ctrlKey || event.metaKey) && event.shiftKey && !event.altKey && event.key.toLowerCase() === "v";
}

/**
 * `Ctrl+Mayús+V` pega el portapapeles como lista `('a', 'b', …)`. El pegado de siempre **no** se
 * toca: convertir en silencio lo que uno pega, aunque sea una columna, sorprendería más de lo que
 * ayuda. Si el texto no se puede convertir (una línea, SQL, una tabla de dos dimensiones) se pega
 * normal.
 */
export function pasteListExtension(): Extension {
  const intent = new PasteIntent();
  return EditorView.domEventHandlers({
    keydown(event) {
      if (isPasteAsListKey(event)) intent.arm(Date.now());
      return false;
    },
    paste(event, view) {
      if (!intent.consume(Date.now())) return false;
      const list = pasteAsList(event.clipboardData?.getData("text/plain") ?? "");
      if (list === null) return false;
      event.preventDefault();
      view.dispatch(view.state.update(view.state.replaceSelection(list), {
        userEvent: "input.paste",
        scrollIntoView: true,
      }));
      return true;
    },
  });
}
