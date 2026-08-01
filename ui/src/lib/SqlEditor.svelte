<script lang="ts">
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { PostgreSQL, sql, type SQLNamespace } from "@codemirror/lang-sql";
  import { Compartment, EditorState, Prec } from "@codemirror/state";
  import { Decoration, EditorView, keymap, type DecorationSet } from "@codemirror/view";
  import { tags } from "@lezer/highlight";
  import { basicSetup } from "codemirror";
  import { untrack } from "svelte";
  import type { ErrorMark } from "./query.svelte";

  let {
    value = $bindable(""),
    schema = undefined,
    errorMark = null,
    readonly = false,
    onrun,
    onrunScript,
    oncancel,
  }: {
    value?: string;
    schema?: SQLNamespace;
    errorMark?: ErrorMark | null;
    readonly?: boolean;
    /** Ctrl+Enter: la selección, o la sentencia donde está el cursor. */
    onrun?: (selection: string, cursor: number) => void;
    /** Ctrl+Shift+Enter: el script entero. */
    onrunScript?: () => void;
    oncancel?: () => void;
  } = $props();

  let element: HTMLDivElement;
  let view: EditorView | null = null;

  const language = new Compartment();
  const marks = new Compartment();

  /**
   * Los colores salen de variables CSS y no de dos temas conmutables porque el resto de la
   * aplicación resuelve el modo oscuro con `prefers-color-scheme`: un tema propio acá sería una
   * segunda fuente de verdad que se desincroniza.
   */
  const highlight = HighlightStyle.define([
    { tag: tags.keyword, color: "var(--cm-keyword)", fontWeight: "500" },
    { tag: [tags.string, tags.special(tags.string)], color: "var(--cm-string)" },
    { tag: [tags.number, tags.bool, tags.null], color: "var(--cm-number)" },
    { tag: [tags.comment, tags.lineComment, tags.blockComment], color: "var(--cm-comment)", fontStyle: "italic" },
    { tag: [tags.typeName, tags.typeOperator], color: "var(--cm-type)" },
    { tag: [tags.function(tags.variableName), tags.standard(tags.name)], color: "var(--cm-function)" },
    { tag: tags.operator, color: "var(--cm-operator)" },
  ]);

  const theme = EditorView.theme({
    "&": { height: "100%", fontSize: "13px", backgroundColor: "transparent" },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.6" },
    ".cm-gutters": {
      backgroundColor: "transparent",
      border: "none",
      color: "var(--cm-gutter)",
    },
    ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--cm-active-line)" },
    ".cm-cursor": { borderLeftColor: "var(--cm-caret)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "var(--cm-selection)",
    },
    ".cm-error-mark": {
      textDecoration: "underline wavy var(--cm-error)",
      textUnderlineOffset: "3px",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--cm-tooltip-bg)",
      border: "1px solid var(--cm-tooltip-border)",
      borderRadius: "6px",
      fontFamily: "var(--font-sans)",
    },
    ".cm-tooltip-autocomplete ul li[aria-selected]": {
      backgroundColor: "var(--cm-selection)",
      color: "inherit",
    },
  });

  /**
   * PostgreSQL cuenta las posiciones en caracteres Unicode y CodeMirror en unidades UTF-16.
   * Coinciden salvo que haya algo fuera del plano básico —un emoji dentro de una cadena— y ahí la
   * marca del error caería corrida.
   */
  function toEditorIndex(text: string, chars: number): number {
    return [...text].slice(0, chars).join("").length;
  }

  function toCharOffset(text: string, index: number): number {
    return [...text.slice(0, index)].length;
  }

  /** Subraya el error abarcando la palabra entera, que es más fácil de ver que un solo carácter. */
  function markOf(text: string, mark: ErrorMark | null): DecorationSet {
    const from = mark ? toEditorIndex(text, mark.at) : 0;
    if (!mark || from >= text.length) return Decoration.none;

    let to = from + 1;
    while (to < text.length && /[\w$."]/.test(text[to])) to += 1;

    return Decoration.set([
      Decoration.mark({ class: "cm-error-mark", attributes: { title: mark.message } }).range(
        from,
        to,
      ),
    ]);
  }

  const shortcuts = Prec.highest(
    keymap.of([
      {
        key: "Mod-Enter",
        preventDefault: true,
        run: (target) => {
          const { from, to } = target.state.selection.main;
          const text = target.state.doc.toString();
          onrun?.(text.slice(from, to), toCharOffset(text, from));
          return true;
        },
      },
      {
        key: "Mod-Shift-Enter",
        preventDefault: true,
        run: () => {
          onrunScript?.();
          return true;
        },
      },
      {
        key: "Escape",
        run: () => {
          oncancel?.();
          return true;
        },
      },
    ]),
  );

  // El editor se crea una sola vez: leer `value` acá sin `untrack` lo reconstruiría en cada tecla,
  // perdiendo el cursor, el historial de deshacer y el foco.
  $effect(() => {
    view = new EditorView({
      parent: element,
      state: EditorState.create({
        doc: untrack(() => value),
        extensions: [
          shortcuts,
          basicSetup,
          language.of(sqlExtension(untrack(() => schema))),
          marks.of(EditorView.decorations.of(Decoration.none)),
          syntaxHighlighting(highlight),
          theme,
          EditorView.lineWrapping,
          EditorState.readOnly.of(untrack(() => readonly)),
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

  function sqlExtension(namespace: SQLNamespace | undefined) {
    return sql({
      dialect: PostgreSQL,
      schema: namespace,
      // Sin esto, escribir `clientes` no completa nada hasta calificarlo con el esquema.
      defaultSchema: "public",
      upperCaseKeywords: true,
    });
  }

  // El esquema llega después de abrir la pestaña, cuando termina la consulta al catálogo.
  $effect(() => {
    view?.dispatch({ effects: language.reconfigure(sqlExtension(schema)) });
  });

  $effect(() => {
    const text = view?.state.doc.toString() ?? "";
    view?.dispatch({
      effects: marks.reconfigure(EditorView.decorations.of(markOf(text, errorMark))),
    });
  });

  // El texto puede cambiar desde afuera (al restaurar del historial); pisar el documento en cada
  // tecleo rompería el cursor, así que solo se toca cuando difiere de verdad.
  $effect(() => {
    if (view && value !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
      });
    }
  });

  export function focus() {
    view?.focus();
  }
</script>

<div class="h-full overflow-hidden [&_.cm-editor]:h-full" bind:this={element}></div>
