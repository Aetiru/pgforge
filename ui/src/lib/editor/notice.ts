import { StateEffect, StateField } from "@codemirror/state";
import { EditorView, showTooltip, type Tooltip } from "@codemirror/view";

/**
 * Un aviso breve pegado al cursor: «no hay nada que expandir», «no hay un alias acá».
 *
 * Va como globo del propio editor y no como cartel de la aplicación: el aviso es sobre lo que se
 * acaba de intentar en ese punto del texto, y se va solo —o apenas se escribe algo— sin que haya
 * que cerrarlo ni que quite la mirada del código.
 */
const setNotice = StateEffect.define<{ pos: number; text: string } | null>();

const noticeField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, transaction) {
    // Escribir algo es seguir con otra cosa: el aviso ya cumplió.
    if (transaction.docChanged) return null;
    for (const effect of transaction.effects) {
      if (!effect.is(setNotice)) continue;
      if (effect.value === null) return null;
      const { pos, text } = effect.value;
      return {
        pos,
        above: true,
        strictSide: false,
        create: () => {
          const dom = document.createElement("div");
          dom.className = "cm-notice";
          dom.textContent = text;
          dom.style.cssText = "padding:4px 8px;font-size:12px;max-width:320px;";
          return { dom };
        },
      };
    }
    return value;
  },
  provide: (field) => showTooltip.from(field),
});

export const noticeExtension = noticeField;

const timers = new WeakMap<EditorView, ReturnType<typeof setTimeout>>();

export function flashNotice(view: EditorView, text: string, ms = 2600) {
  const previous = timers.get(view);
  if (previous) clearTimeout(previous);
  view.dispatch({ effects: setNotice.of({ pos: view.state.selection.main.head, text }) });
  timers.set(
    view,
    setTimeout(() => {
      // El editor pudo cerrarse mientras tanto (se cambió de pestaña).
      try {
        view.dispatch({ effects: setNotice.of(null) });
      } catch {
        /* destruido */
      }
    }, ms),
  );
}
