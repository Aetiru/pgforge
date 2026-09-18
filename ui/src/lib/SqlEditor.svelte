<script lang="ts">
  import {
    acceptCompletion,
    completionStatus,
    hasNextSnippetField,
    hasPrevSnippetField,
  } from "@codemirror/autocomplete";
  import { indentLess, indentMore } from "@codemirror/commands";
  import { syntaxHighlighting } from "@codemirror/language";
  import { searchPanelOpen } from "@codemirror/search";
  import { PostgreSQL, sql, type SQLNamespace } from "@codemirror/lang-sql";
  import { Compartment, EditorState, Prec } from "@codemirror/state";
  import { EditorView, hoverTooltip, keymap } from "@codemirror/view";
  import { basicSetup } from "codemirror";
  import { untrack } from "svelte";
  import { sqlHighlight } from "./sql-highlight";
  import { sqlNesting } from "./sql-nested";
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
    readonly = false,
    initialSelection = null,
    initialTopPos = null,
    onrun,
    onrunScript,
    oncancel,
    onsave,
    onformat,
    onreveal,
    onposition,
  }: {
    value?: string;
    schema?: SQLNamespace;
    /** Los mismos nombres en plano, para completar las columnas del `FROM` sin calificar. */
    relations?: SchemaRelation[];
    errorMark?: ErrorMark | null;
    readonly?: boolean;
    /** Cursor y scroll con los que nace el editor —lo que dejó `ondetach` la vez anterior. */
    initialSelection?: { anchor: number; head: number } | null;
    /** Posición del documento que quedaba arriba del todo. Guardar el `scrollTop` en píxeles no
     *  alcanza: CodeMirror virtualiza y recién mide el alto real de cada línea cuando la dibuja, así
     *  que un píxel fijo asignado antes de esa medición queda pisado. Una posición del documento no
     *  depende de esa medición —CodeMirror la resuelve sola, cuando esté lista, vía
     *  `scrollIntoView`—. */
    initialTopPos?: number | null;
    /** Ctrl+Enter: la selección, o la sentencia donde está el cursor. */
    onrun?: (selection: string, cursor: number) => void;
    /** Ctrl+Shift+Enter: el script entero. */
    onrunScript?: () => void;
    oncancel?: () => void;
    /** Ctrl+S guarda donde ya se había guardado; Ctrl+Shift+S siempre pregunta. */
    onsave?: (askPath: boolean) => void;
    /** Ctrl+Mayús+F: la selección si hay una, o el documento entero. */
    onformat?: (selection: string, cursor: number) => void;
    /** `Ctrl`+clic sobre una tabla del `FROM`/`JOIN`: la relación resuelta, para revelarla en el
     *  árbol. `null` cuando el clic no cayó sobre nada reconocible. */
    onreveal?: (relation: SchemaRelation | null) => void;
    /**
     * Cursor y scroll, cada vez que cambian —no solo al desmontar—, para que `QueryPanel` los guarde
     * en la pestaña y este mismo editor nazca donde se había quedado la próxima vez que se abra.
     *
     * Capturarlos recién en el `return` del `$effect` de creación parecía alcanzar, pero para cuando
     * ese cleanup corre —Svelte ya está desarmando el árbol viejo del `{#key}` de `App.svelte`— el
     * contenedor puede haber perdido su tamaño de layout, y ahí `scrollDOM.scrollTop` lee `0` aunque
     * un instante antes valiera miles de píxeles. Iistoría corriente en cada cambio evita depender de
     * ese orden de desmontaje. Guardar en cada cambio —scroll o selección— deja siempre un valor
     * reciente y bueno, sin importar en qué momento se desarme el editor.
     */
    onposition?: (state: { anchor: number; head: number; topPos: number }) => void;
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
    // `basicSetup` tiñe la línea del cursor con un color propio del tema base; se apaga en vez de
    // sacar `highlightActiveLine`, que habría obligado a armar la lista de extensiones a mano.
    ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "transparent" },
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
      // Sin esto nadie atiende el tabulador —`basicSetup` no trae `indentWithTab`— y el navegador
      // se lo lleva al siguiente elemento enfocable, que es la grilla de resultados. Antes de
      // indentar cede ante lo que el tabulador ya significaba donde el cursor está parado: adentro
      // de una expansión abierta salta al hueco que sigue, y con la lista de sugerencias abierta
      // acepta la elegida, que es lo que la mano espera de cualquier editor. El precio es que con
      // el foco en el editor ya no se sale de él con el teclado, y se paga a propósito: esto es una
      // ventana de escritorio y no un formulario que se recorre tabulando.
      {
        key: "Tab",
        run: (target) => {
          if (hasNextSnippetField(target.state)) return false;
          if (acceptCompletion(target)) return true;
          return indentMore(target);
        },
        shift: (target) => {
          if (hasPrevSnippetField(target.state)) return false;
          return indentLess(target);
        },
      },
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

  /** Posición de arriba del todo, en documento y no en píxeles —ver el comentario de `onposition`. */
  function reportPosition(editorView: EditorView) {
    const { anchor, head } = editorView.state.selection.main;
    const topPos = editorView.lineBlockAtHeight(editorView.scrollDOM.scrollTop).from;
    onposition?.({ anchor, head, topPos });
  }

  // El editor se crea una sola vez: leer `value` acá sin `untrack` lo reconstruiría en cada tecla,
  // perdiendo el cursor, el historial de deshacer y el foco.
  $effect(() => {
    const startDoc = untrack(() => value);
    const startSelection = untrack(() => initialSelection);
    // El documento guardado puede ser más corto que cuando se anotó la selección —se restauró
    // desde el historial, o cambió de base y perdió lo escrito—, y CodeMirror tira si el cursor cae
    // más allá del final.
    const selection = startSelection
      ? {
          anchor: Math.min(startSelection.anchor, startDoc.length),
          head: Math.min(startSelection.head, startDoc.length),
        }
      : undefined;

    view = new EditorView({
      parent: element,
      state: EditorState.create({
        doc: startDoc,
        selection,
        extensions: [
          shortcuts,
          basicSetup,
          language.of(sqlExtension(untrack(() => schema))),
          errorMarkField,
          revealHandler,
          columnHover,
          syntaxHighlighting(sqlHighlight),
          sqlNesting,
          theme,
          EditorView.lineWrapping,
          EditorState.readOnly.of(untrack(() => readonly)),
          // El scroll no dispara `update`, así que se escucha aparte; es lo único que hace que
          // `onposition` quede al día mientras se lee sin tocar el cursor.
          EditorView.domEventHandlers({
            scroll: (_event, editorView) => reportPosition(editorView),
          }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) value = update.state.doc.toString();
            if (update.selectionSet) reportPosition(update.view);
          }),
        ],
      }),
    });

    // Vía `scrollIntoView` y no asignando `scrollDOM.scrollTop` a mano: CodeMirror todavía no midió
    // el alto real de las líneas fuera de vista en este primer render, y un píxel fijo puesto antes
    // de esa medición queda pisado apenas termina. Una posición del documento no tiene ese problema
    // —CodeMirror la resuelve cuando le toca, sea cual sea el orden—.
    //
    // Aun así, un salto lejano —a la línea 200 de una función larga, recién montado el editor— pide
    // dos pasadas. La primera renderiza alrededor de una altura estimada, todavía sin medir esa zona
    // del documento; recién ahí CodeMirror mide las líneas reales que acaba de dibujar, y esa medida
    // puede correr bastante el resultado si hay líneas envueltas de alto disparejo de por medio. La
    // segunda pasada, ya con esa medida hecha, cae en el lugar justo.
    const startTopPos = untrack(() => initialTopPos);
    if (startTopPos !== null) {
      const target = Math.min(startTopPos, view.state.doc.length);
      const scrollTo = EditorView.scrollIntoView(target, { y: "start" });
      view.dispatch({ effects: scrollTo });
      const restored = view;
      requestAnimationFrame(() => {
        if (view === restored) view.dispatch({ effects: scrollTo });
      });
    }

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
