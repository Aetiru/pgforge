<script lang="ts">
  import { completionStatus } from "@codemirror/autocomplete";
  import { syntaxHighlighting } from "@codemirror/language";
  import { searchPanelOpen } from "@codemirror/search";
  import { PostgreSQL, sql, type SQLNamespace } from "@codemirror/lang-sql";
  import { Compartment, EditorState, Prec } from "@codemirror/state";
  import { Decoration, EditorView, keymap, type DecorationSet } from "@codemirror/view";
  import { basicSetup } from "codemirror";
  import { untrack } from "svelte";
  import { sqlHighlight } from "./sql-highlight";
  import { columnCompletion } from "./sql-complete";
  import { sqlFont } from "./editor.svelte";
  import type { SchemaRelation } from "./ipc";
  import type { ErrorMark } from "./query.svelte";

  let {
    value = $bindable(""),
    schema = undefined,
    relations = [],
    errorMark = null,
    readonly = false,
    onrun,
    onrunScript,
    oncancel,
    onsave,
  }: {
    value?: string;
    schema?: SQLNamespace;
    /** Los mismos nombres en plano, para completar las columnas del `FROM` sin calificar. */
    relations?: SchemaRelation[];
    errorMark?: ErrorMark | null;
    readonly?: boolean;
    /** Ctrl+Enter: la selección, o la sentencia donde está el cursor. */
    onrun?: (selection: string, cursor: number) => void;
    /** Ctrl+Shift+Enter: el script entero. */
    onrunScript?: () => void;
    oncancel?: () => void;
    /** Ctrl+S guarda donde ya se había guardado; Ctrl+Shift+S siempre pregunta. */
    onsave?: (askPath: boolean) => void;
  } = $props();

  let element: HTMLDivElement;
  let view: EditorView | null = null;

  const language = new Compartment();
  const marks = new Compartment();

  const theme = EditorView.theme({
    // El tamaño sale de la variable que maneja `sqlFont`: lo cambia el usuario y vale para todo el
    // SQL de la aplicación, no solo para este editor.
    "&": { height: "100%", fontSize: "var(--sql-font-size)", backgroundColor: "transparent" },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.6" },
    ".cm-content": { paddingBlock: "8px" },
    ".cm-gutters": {
      backgroundColor: "transparent",
      border: "none",
      color: "var(--cm-gutter)",
    },
    ".cm-lineNumbers .cm-gutterElement": { paddingLeft: "10px", minWidth: "34px" },
    /* El panel de búsqueda de CodeMirror viene con su propio gris: se lo iguala al de la ventana. */
    ".cm-panels": {
      backgroundColor: "var(--cm-tooltip-bg)",
      color: "inherit",
      borderColor: "var(--cm-tooltip-border)",
    },
    ".cm-panels input, .cm-panels button": { fontFamily: "var(--font-sans)", fontSize: "12px" },
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
        // Este keymap es `Prec.highest`, así que sin la guarda se comía el Escape que cierra el
        // autocompletado y el panel de búsqueda: cancelaba la consulta y la lista quedaba abierta.
        // Devolver `false` deja que lo atienda el manejador que corresponde.
        key: "Escape",
        run: (target) => {
          if (completionStatus(target.state) !== null) return false;
          if (searchPanelOpen(target.state)) return false;
          oncancel?.();
          return true;
        },
      },
      {
        key: "Mod-s",
        preventDefault: true,
        run: () => {
          onsave?.(false);
          return true;
        },
      },
      {
        key: "Mod-Shift-s",
        preventDefault: true,
        run: () => {
          onsave?.(true);
          return true;
        },
      },
      // El zoom del SQL con los atajos de siempre. Van acá y no en la ventana porque `Ctrl -` sobre
      // un editor con foco lo tiene que atender el editor; el resto de la aplicación no cambia de
      // tamaño con ellos.
      {
        key: "Mod-=",
        preventDefault: true,
        run: () => {
          sqlFont.bigger();
          return true;
        },
      },
      {
        key: "Mod-Shift-=",
        preventDefault: true,
        run: () => {
          sqlFont.bigger();
          return true;
        },
      },
      {
        key: "Mod--",
        preventDefault: true,
        run: () => {
          sqlFont.smaller();
          return true;
        },
      },
      {
        key: "Mod-0",
        preventDefault: true,
        run: () => {
          sqlFont.reset();
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
          syntaxHighlighting(sqlHighlight),
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
    return [
      sql({
        dialect: PostgreSQL,
        schema: namespace,
        // Sin esto, escribir `clientes` no completa nada hasta calificarlo con el esquema.
        defaultSchema: "public",
        upperCaseKeywords: true,
      }),
      // Sumada a la del dialecto, no en su lugar: esa sigue resolviendo `tabla.` y las palabras
      // clave, y esta agrega lo único que le falta, las columnas del `FROM` sin calificar.
      // Las relaciones se leen por función para que el cambio de base no exija reconfigurar.
      PostgreSQL.language.data.of({ autocomplete: columnCompletion(() => relations) }),
    ];
  }

  // El esquema llega después de abrir la pestaña, cuando termina la consulta al catálogo, y cambia
  // entero al cambiar de base.
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

  // CodeMirror mide el ancho de un carácter una sola vez: si la letra cambia y nadie le avisa, el
  // cursor y el salto de línea quedan calculados con el tamaño viejo.
  $effect(() => {
    sqlFont.size;
    view?.requestMeasure();
  });

  export function focus() {
    view?.focus();
  }

  /**
   * Lo seleccionado y dónde está el cursor, ahora mismo.
   *
   * Se pregunta en vez de avisarse: los botones de la barra corren mucho después del último tecleo,
   * y una copia guardada en el panel queda en donde estaba el cursor la vez anterior —con el editor
   * recién abierto, en cero, o sea siempre la primera sentencia—.
   */
  export function selection(): { text: string; cursor: number } {
    if (!view) return { text: "", cursor: 0 };
    const { from, to } = view.state.selection.main;
    const text = view.state.doc.toString();
    return { text: text.slice(from, to), cursor: toCharOffset(text, from) };
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="h-full overflow-hidden [&_.cm-editor]:h-full"
  bind:this={element}
  onwheel={(event) => {
    // Ctrl + rueda es el gesto que todos prueban antes de buscar el botón.
    if (!event.ctrlKey) return;
    event.preventDefault();
    if (event.deltaY < 0) sqlFont.bigger();
    else sqlFont.smaller();
  }}
></div>
