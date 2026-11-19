/**
 * Sugerir desde el historial: mientras se escribe una sentencia, un texto gris continúa con algo que
 * ya se ejecutó antes y que empieza igual. `Tab` lo acepta entero; seguir escribiendo lo que dice lo
 * va consumiendo, y cualquier otra cosa lo descarta.
 *
 * Solo aparece cuando el cursor está **al final de la sentencia** (lo que sigue es espacio) y lo
 * escrito ya tiene cuerpo: con tres letras casi cualquier consulta empieza igual y el fantasma sería
 * ruido. El historial lo busca el núcleo (`history_suggest`): solo ejecuciones exitosas, que no
 * vengan de un diálogo.
 */

import { completionStatus } from "@codemirror/autocomplete";
import { Facet, StateEffect, type EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from "@codemirror/view";

/** Menos que esto es casi cualquier consulta: el fantasma sería ruido. */
export const MIN_PREFIX = 12;
export const GHOST_DELAY_MS = 450;
/** Un prefijo enorme no es lo que se busca en el historial y viajaría entero al núcleo. */
const MAX_PREFIX = 2000;

// ---------------------------------------------------------------------------------------------
// Lo puro
// ---------------------------------------------------------------------------------------------

/**
 * Lo que se le pide al historial: la sentencia desde su inicio hasta el cursor. `null` si no
 * corresponde sugerir: el cursor no está al final de la sentencia, o lo escrito es corto.
 *
 * `statementStart` y el largo de la sentencia vienen del núcleo (el mismo que usa ejecutar).
 */
export function ghostPrefix(doc: string, cursor: number, statementStart: number, statementEnd: number): string | null {
  if (cursor < statementStart || cursor > doc.length) return null;
  // Lo que queda después del cursor dentro de la sentencia tiene que ser espacio o el `;` final.
  const after = doc.slice(cursor, Math.max(cursor, statementEnd));
  if (!/^[\s;]*$/.test(after)) return null;
  // El cursor tras un `;` ya es de la sentencia siguiente: no hay nada que continuar.
  if (doc.slice(statementStart, cursor).trimEnd().endsWith(";")) return null;

  const prefix = doc.slice(statementStart, cursor);
  if (prefix.trim().length < MIN_PREFIX || prefix.length > MAX_PREFIX) return null;
  return prefix;
}

/** Lo que falta escribir de la propuesta, o `null` si no continúa el prefijo o no agrega nada. */
export function ghostRemainder(suggestion: string, prefix: string): string | null {
  if (!suggestion.startsWith(prefix) || suggestion.length <= prefix.length) return null;
  return suggestion.slice(prefix.length);
}

/** Lo que se dibuja: solo hasta el primer salto de línea, con `…` si había más. */
export function ghostDisplay(remainder: string): string {
  const newline = remainder.indexOf("\n");
  if (newline === -1) return remainder;
  return `${remainder.slice(0, newline).trimEnd()}…`;
}

export interface Ghost {
  /** Dónde se dibuja: la posición del cursor cuando se pidió. */
  pos: number;
  /** Lo que falta, entero: es lo que inserta `Tab`. */
  rest: string;
}

/**
 * El fantasma después de una edición. Si lo escrito es justo lo que venía, se consume (el usuario
 * está tecleando la propuesta); cualquier otra cosa lo descarta.
 */
export function advanceGhost(ghost: Ghost, change: { at: number; removed: number; inserted: string }): Ghost | null {
  if (change.removed !== 0 || change.at !== ghost.pos || change.inserted === "") return null;
  if (!ghost.rest.startsWith(change.inserted)) return null;
  const rest = ghost.rest.slice(change.inserted.length);
  return rest === "" ? null : { pos: ghost.pos + change.inserted.length, rest };
}

// ---------------------------------------------------------------------------------------------
// La extensión
// ---------------------------------------------------------------------------------------------

const setGhost = StateEffect.define<Ghost | null>();

class GhostWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  eq(other: GhostWidget) {
    return other.text === this.text;
  }
  toDOM() {
    const span = document.createElement("span");
    span.className = "cm-ghost-suggest";
    span.textContent = this.text;
    span.setAttribute("aria-hidden", "true");
    return span;
  }
  // El fantasma no se puede clicar ni seleccionar: el cursor lo atraviesa.
  ignoreEvent() {
    return true;
  }
}

export interface GhostDeps {
  statementAt: (text: string, cursor: number) => Promise<{ text: string; offset: number } | null>;
  suggest: (prefix: string) => Promise<string[]>;
  enabled: () => boolean;
}

const ghostTheme = EditorView.baseTheme({
  ".cm-ghost-suggest": { color: "var(--cm-ghost)", opacity: "0.75", fontStyle: "italic", pointerEvents: "none" },
});

function toEditorIndex(text: string, chars: number): number {
  return [...text].slice(0, chars).join("").length;
}

/**
 * Las dependencias viajan por una faceta y no por el cierre del plugin: hay un editor por pestaña y
 * el `Tab` necesita encontrar **este** plugin en **esta** vista (`view.plugin`). Con un plugin nuevo
 * por editor, el último creado pisaba al resto y aceptar la propuesta dejaba de andar en las demás.
 */
const ghostDeps = Facet.define<GhostDeps, GhostDeps | null>({ combine: (values) => values[0] ?? null });

const ghostPlugin =
  ViewPlugin.fromClass(
    class {
      ghost: Ghost | null = null;
      decorations: DecorationSet = Decoration.none;
      private timer: ReturnType<typeof setTimeout> | undefined;
      private serial = 0;

      constructor(private view: EditorView) {}

      update(update: ViewUpdate) {
        let next = this.ghost;

        for (const tr of update.transactions) {
          for (const effect of tr.effects) if (effect.is(setGhost)) next = effect.value;
        }

        if (update.docChanged) {
          // Una sola inserción pegada al fantasma se consume; todo lo demás lo descarta.
          let advanced: Ghost | null = null;
          let simple = false;
          if (this.ghost && !update.transactions.some((tr) => tr.effects.some((e) => e.is(setGhost)))) {
            let count = 0;
            update.changes.iterChanges((fromA, toA, _fromB, _toB, inserted) => {
              count += 1;
              if (count === 1 && this.ghost) {
                advanced = advanceGhost(this.ghost, { at: fromA, removed: toA - fromA, inserted: inserted.toString() });
              }
            });
            simple = count === 1;
          }
          next = simple ? advanced : next === this.ghost ? null : next;
          this.schedule();
        } else if (update.selectionSet && next && next === this.ghost) {
          // Mover el cursor lejos del fantasma lo descarta; quedarse en el lugar lo conserva.
          if (update.state.selection.main.head !== next.pos || !update.state.selection.main.empty) next = null;
        }

        this.ghost = next;
        this.decorations = this.build(update.state);
      }

      private build(state: EditorState): DecorationSet {
        if (!this.ghost) return Decoration.none;
        // La lista de sugerencias abierta tiene la palabra: dos propuestas a la vez confunden.
        if (completionStatus(state) !== null) return Decoration.none;
        if (this.ghost.pos > state.doc.length) return Decoration.none;
        return Decoration.set([
          Decoration.widget({ widget: new GhostWidget(ghostDisplay(this.ghost.rest)), side: 1 }).range(this.ghost.pos),
        ]);
      }

      destroy() {
        clearTimeout(this.timer);
        this.serial += 1;
      }

      private schedule() {
        clearTimeout(this.timer);
        const serial = ++this.serial;
        this.timer = setTimeout(() => void this.fetch(serial), GHOST_DELAY_MS);
      }

      private async fetch(serial: number) {
        const { view } = this;
        const deps = view.state.facet(ghostDeps);
        if (!deps || !view.dom.isConnected || !deps.enabled() || view.state.readOnly) return;
        const selection = view.state.selection.main;
        if (!selection.empty || view.state.selection.ranges.length > 1) return;
        if (completionStatus(view.state) !== null) return;

        const doc = view.state.doc;
        const text = doc.toString();
        const head = selection.head;

        let statement: { text: string; offset: number } | null;
        try {
          statement = await deps.statementAt(text, head);
        } catch {
          return;
        }
        if (serial !== this.serial || view.state.doc !== doc || !statement) return;

        const start = toEditorIndex(text, statement.offset);
        const prefix = ghostPrefix(text, head, start, start + statement.text.length);
        if (prefix === null) return;

        let suggestions: string[];
        try {
          suggestions = await deps.suggest(prefix);
        } catch {
          return;
        }
        // Mientras llegaba la respuesta el usuario pudo seguir escribiendo o mover el cursor.
        if (serial !== this.serial || view.state.doc !== doc || view.state.selection.main.head !== head) return;

        for (const suggestion of suggestions) {
          const rest = ghostRemainder(suggestion, prefix);
          if (rest !== null) {
            view.dispatch({ effects: setGhost.of({ pos: head, rest }) });
            return;
          }
        }
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );

export function ghostSuggest(deps: GhostDeps): Extension {
  return [ghostDeps.of(deps), ghostPlugin, ghostTheme];
}

/** Inserta lo que falta de la propuesta. Devuelve `true` si había una (para el `Tab`). */
export function acceptGhost(view: EditorView): boolean {
  const ghost = view.plugin(ghostPlugin)?.ghost;
  if (!ghost || view.state.readOnly) return false;
  const { head, empty } = view.state.selection.main;
  if (!empty || head !== ghost.pos) return false;

  view.dispatch({
    changes: { from: ghost.pos, insert: ghost.rest },
    selection: { anchor: ghost.pos + ghost.rest.length },
    effects: setGhost.of(null),
    userEvent: "input.complete",
    scrollIntoView: true,
  });
  return true;
}

/** Descarta la propuesta. Devuelve `true` si había una (para el `Escape`). */
export function dismissGhost(view: EditorView): boolean {
  if (!view.plugin(ghostPlugin)?.ghost) return false;
  view.dispatch({ effects: setGhost.of(null) });
  return true;
}
