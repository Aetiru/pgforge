<script lang="ts">
  import { history, historyKeymap, defaultKeymap } from "@codemirror/commands";
  import { syntaxHighlighting } from "@codemirror/language";
  import { PostgreSQL, sql } from "@codemirror/lang-sql";
  import { EditorState, RangeSetBuilder } from "@codemirror/state";
  import {
    Decoration,
    EditorView,
    ViewPlugin,
    keymap,
    placeholder as placeholderExt,
    type DecorationSet,
    type ViewUpdate,
  } from "@codemirror/view";
  import { untrack } from "svelte";
  import { findHoles, type HoleKind } from "./snippet-dialog";
  import { sqlHighlight } from "./sql-highlight";

  /**
   * El cuerpo de una abreviatura, escrito con los colores del SQL y con los `${…}` resaltados.
   *
   * Es un `textarea` con ínfulas: sin autocompletado del catálogo ni números de línea, porque lo que
   * se escribe acá es una plantilla y no una consulta contra ninguna base. Lo que sí hace es pintar
   * distinto cada clase de hueco, para que se vea de un vistazo cuál es un salto de tabulador, cuál
   * una pregunta y cuál un valor que se llena solo.
   *
   * No captura el tabulador: en un diálogo, Tab es moverse al campo siguiente.
   */
  let {
    value = $bindable(""),
    placeholder = "",
  }: { value?: string; placeholder?: string } = $props();

  let element: HTMLDivElement;
  let view: EditorView | null = null;

  const CLASS: Record<HoleKind, string> = {
    hole: "cm-snip-hole",
    variable: "cm-snip-var",
    list: "cm-snip-var",
    typed: "cm-snip-var",
    auto: "cm-snip-auto",
  };

  function decorate(state: EditorState): DecorationSet {
    const builder = new RangeSetBuilder<Decoration>();
    for (const hole of findHoles(state.doc.toString())) {
      builder.add(hole.from, hole.to, Decoration.mark({ class: CLASS[hole.kind] }));
    }
    return builder.finish();
  }

  const holes = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = decorate(view.state);
      }
      update(update: ViewUpdate) {
        if (update.docChanged) this.decorations = decorate(update.state);
      }
    },
    { decorations: (plugin) => plugin.decorations },
  );

  const theme = EditorView.theme({
    "&": {
      fontSize: "var(--sql-font-size)",
      backgroundColor: "transparent",
      border: "1px solid var(--cm-tooltip-border)",
      borderRadius: "6px",
    },
    "&.cm-focused": { outline: "2px solid var(--cm-selection)" },
    ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "var(--sql-leading, 1.65)", maxHeight: "16rem" },
    ".cm-content": { padding: "8px", minHeight: "8rem" },
    ".cm-cursor": { borderLeftColor: "var(--cm-caret)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "var(--cm-selection)",
    },
    ".cm-snip-hole": { color: "var(--cm-snip-hole)", fontWeight: "600" },
    ".cm-snip-var": { color: "var(--cm-snip-var)", fontWeight: "600" },
    ".cm-snip-auto": { color: "var(--cm-snip-auto)", fontWeight: "600" },
    ".cm-placeholder": { color: "var(--cm-ghost)" },
  });

  // Se crea una sola vez: leer `value` sin `untrack` lo reconstruiría en cada tecla y se perdería el
  // cursor y el historial de deshacer.
  $effect(() => {
    const start = untrack(() => value);
    view = new EditorView({
      parent: element,
      state: EditorState.create({
        doc: start,
        extensions: [
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          sql({ dialect: PostgreSQL }),
          syntaxHighlighting(sqlHighlight),
          holes,
          theme,
          EditorView.lineWrapping,
          placeholderExt(untrack(() => placeholder)),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) value = update.state.doc.toString();
          }),
        ],
      }),
    });
    return () => {
      view?.destroy();
      view = null;
    };
  });

  // El texto puede cambiar desde afuera (al convertir literales); solo se pisa cuando difiere.
  $effect(() => {
    if (view && value !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
    }
  });

  export function focus() {
    view?.focus();
  }
</script>

<div bind:this={element} class="select-text" data-snippet-body></div>
