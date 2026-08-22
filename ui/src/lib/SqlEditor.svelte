<script lang="ts">
  import { completionStatus } from "@codemirror/autocomplete";
  import { syntaxHighlighting } from "@codemirror/language";
  import { searchPanelOpen } from "@codemirror/search";
  import { PostgreSQL, sql, type SQLNamespace } from "@codemirror/lang-sql";
  import { Compartment, EditorState, Prec } from "@codemirror/state";
  import { EditorView, hoverTooltip, keymap } from "@codemirror/view";
  import { basicSetup } from "codemirror";
  import { untrack } from "svelte";
  import { sqlHighlight } from "./sql-highlight";
  import { sqlNesting } from "./sql-nested";
  import { activeMarkField, activeMarkOf, setActiveMark, type ActiveRange } from "./sql-active-mark";
  import { errorMarkField, markOf, setErrorMark } from "./sql-error-mark";
  import { expandBinding, snippetCompletions } from "./sql-snippet";
  import { snippets } from "./snippets.svelte";
  import {
    columnCompletion,
    hoverInfo,
    qualifiedNameAt,
    relationAt,
    tablesInScope,
  } from "./sql-complete";
  import { sqlFont } from "./editor.svelte";
  import type { SchemaRelation } from "./ipc";
  import type { ErrorMark } from "./query.svelte";

  let {
    value = $bindable(""),
    schema = undefined,
    relations = [],
    errorMark = null,
    activeRange = null,
    readonly = false,
    onrun,
    onrunScript,
    oncancel,
    onsave,
    onformat,
    oncursor,
    onreveal,
  }: {
    value?: string;
    schema?: SQLNamespace;
    /** Los mismos nombres en plano, para completar las columnas del `FROM` sin calificar. */
    relations?: SchemaRelation[];
    errorMark?: ErrorMark | null;
    /** La sentencia que `Ctrl+Enter` va a correr, ya resuelta por `QueryPanel`. */
    activeRange?: ActiveRange | null;
    readonly?: boolean;
    /** Ctrl+Enter: la selección, o la sentencia donde está el cursor. */
    onrun?: (selection: string, cursor: number) => void;
    /** Ctrl+Shift+Enter: el script entero. */
    onrunScript?: () => void;
    oncancel?: () => void;
    /** Ctrl+S guarda donde ya se había guardado; Ctrl+Shift+S siempre pregunta. */
    onsave?: (askPath: boolean) => void;
    /** Ctrl+Mayús+F: la selección si hay una, o el documento entero. */
    onformat?: (selection: string, cursor: number) => void;
    /** El cursor se movió, o el documento cambió: `QueryPanel` decide con esto si hay que volver a
     *  pedir cuál es la sentencia activa. */
    oncursor?: (selection: string, cursor: number) => void;
    /** `Ctrl`+clic sobre una tabla del `FROM`/`JOIN`: la relación resuelta, para revelarla en el
     *  árbol. `null` cuando el clic no cayó sobre nada reconocible. */
    onreveal?: (relation: SchemaRelation | null) => void;
  } = $props();

  let element: HTMLDivElement;
  let view: EditorView | null = null;

  const language = new Compartment();

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
    // La sentencia que `Ctrl+Enter` va a correr. Va aparte de `.cm-activeLine`: una tapa la línea
    // del cursor y la otra la sentencia entera, y las dos pueden estar a la vista a la vez.
    ".cm-active-statement": { backgroundColor: "var(--cm-active-statement)" },
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
    ".cm-hover-info": { padding: "6px 8px", maxWidth: "360px", fontSize: "12px" },
    ".cm-hover-type": { fontFamily: "var(--font-mono)", fontWeight: "600" },
    ".cm-hover-comment": { marginTop: "4px", color: "var(--cm-gutter)" },
  });

  /**
   * PostgreSQL cuenta las posiciones en caracteres Unicode y CodeMirror en unidades UTF-16.
   * Coinciden salvo que haya algo fuera del plano básico —un emoji dentro de una cadena— y ahí la
   * marca del error caería corrida.
   */
  function toCharOffset(text: string, index: number): number {
    return [...text.slice(0, index)].length;
  }

  const shortcuts = Prec.highest(
    keymap.of([
      // Primero de la lista, pero devuelve `false` cuando no le toca (ver `sql-snippet`): así el
      // tabulador sigue saltando entre los huecos de una expansión ya abierta.
      expandBinding(() => snippets.items),
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
      {
        key: "Mod-Shift-f",
        preventDefault: true,
        run: (target) => {
          const { from, to } = target.state.selection.main;
          const text = target.state.doc.toString();
          onformat?.(text.slice(from, to), toCharOffset(text, from));
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

  /**
   * `Ctrl`+clic sobre una tabla del `FROM`/`JOIN` la revela en el árbol.
   *
   * Solo intercepta el clic cuando de verdad resuelve una tabla: devolver `false` en cualquier otro
   * caso deja que el clic mueva el cursor como siempre, que es lo que se espera si el `Ctrl`+clic no
   * cayó sobre nada reconocible.
   */
  const revealHandler = EditorView.domEventHandlers({
    mousedown(event, editorView) {
      if (!(event.ctrlKey || event.metaKey)) return false;

      const pos = editorView.posAtCoords({ x: event.clientX, y: event.clientY });
      if (pos === null) return false;

      const text = editorView.state.doc.toString();
      const qualified = qualifiedNameAt(text, pos);
      if (!qualified) return false;

      const found = relationAt(relations, tablesInScope(text, pos), qualified);
      if (!found) return false;

      onreveal?.(found);
      return true;
    },
  });

  /**
   * Tipo y comentario de una columna al pasar el mouse; comentario de la tabla si no es una
   * columna. `hoverInfo` decide qué hay para mostrar; acá solo se arma el globo.
   */
  const columnHover = hoverTooltip((editorView, pos) => {
    const text = editorView.state.doc.toString();
    const qualified = qualifiedNameAt(text, pos);
    if (!qualified) return null;

    const info = hoverInfo(relations, tablesInScope(text, pos), qualified);
    if (!info) return null;

    return {
      pos: qualified.from,
      end: qualified.to,
      create: () => {
        const dom = document.createElement("div");
        dom.className = "cm-hover-info";

        if (info.kind === "column") {
          const type = document.createElement("div");
          type.className = "cm-hover-type";
          type.textContent = `${info.table}.${info.column.name}: ${info.column.typeName}`;
          dom.appendChild(type);
        }

        const comment = info.kind === "column" ? info.column.comment : info.relation.comment;
        if (comment) {
          const commentDiv = document.createElement("div");
          commentDiv.className = "cm-hover-comment";
          commentDiv.textContent = comment;
          dom.appendChild(commentDiv);
        }

        return { dom };
      },
    };
  });

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
          errorMarkField,
          activeMarkField,
          revealHandler,
          columnHover,
          syntaxHighlighting(sqlHighlight),
          sqlNesting,
          theme,
          EditorView.lineWrapping,
          EditorState.readOnly.of(untrack(() => readonly)),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) value = update.state.doc.toString();
            // `QueryPanel` decide con esto si hay que volver a preguntar cuál es la sentencia
            // activa: tanto escribir como mover el cursor pueden cambiar la respuesta.
            if (update.docChanged || update.selectionSet) {
              const { from, to } = update.state.selection.main;
              const text = update.state.doc.toString();
              oncursor?.(text.slice(from, to), toCharOffset(text, from));
            }
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
      // Y las abreviaturas, que si no habría que recordar de memoria.
      PostgreSQL.language.data.of({ autocomplete: snippetCompletions(() => snippets.items) }),
    ];
  }

  // El esquema llega después de abrir la pestaña, cuando termina la consulta al catálogo, y cambia
  // entero al cambiar de base.
  $effect(() => {
    view?.dispatch({ effects: language.reconfigure(sqlExtension(schema)) });
  });

  // Se manda como efecto y no como reconfiguración de un compartimento: el campo lo mapea con cada
  // cambio del documento, así que la marca sigue a su palabra en vez de quedar clavada en un
  // desplazamiento que el próximo borrado deja afuera del texto (ver `sql-error-mark`).
  $effect(() => {
    const text = view?.state.doc.toString() ?? "";
    view?.dispatch({ effects: setErrorMark.of(markOf(text, errorMark)) });
  });

  // Mismo motivo que el efecto del error: se manda como efecto y no como reconfiguración de un
  // compartimento, para que el campo mapee la marca con cada cambio del documento en vez de
  // quedar clavada en un desplazamiento que ya no es el mismo texto.
  $effect(() => {
    const text = view?.state.doc.toString() ?? "";
    view?.dispatch({ effects: setActiveMark.of(activeMarkOf(text, activeRange)) });
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

  /**
   * Reemplaza lo seleccionado —o el documento entero, si no hay selección— con `text`, y deja el
   * cursor en `cursorInText`: un desplazamiento UTF-16 dentro de `text`, no del script del núcleo,
   * porque quien llama ya lo calculó sobre el propio texto formateado (ver `format-cursor.ts`).
   *
   * Es la función que `QueryPanel` usa para devolver el resultado de formatear: `SqlEditor` no
   * pide el SQL al núcleo, pero sí sabe aplicar lo que ya volvió.
   */
  export function applyFormat(text: string, cursorInText: number) {
    if (!view) return;
    const { from, to } = view.state.selection.main;
    const hasSelection = from !== to;
    const rangeFrom = hasSelection ? from : 0;
    const rangeTo = hasSelection ? to : view.state.doc.length;

    view.dispatch({
      changes: { from: rangeFrom, to: rangeTo, insert: text },
      selection: { anchor: rangeFrom + cursorInText },
      scrollIntoView: true,
    });
    view.focus();
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
