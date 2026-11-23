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
  import {
    EditorView,
    hoverTooltip,
    keymap,
    tooltips,
    type KeyBinding,
  } from "@codemirror/view";
  import { basicSetup } from "codemirror";
  import { untrack } from "svelte";
  import { t } from "./i18n.svelte";
  import { sqlHighlight } from "./sql-highlight";
  import { sqlNesting } from "./sql-nested";
  import { errorMarkField, markOf, setErrorMark, toEditorIndex } from "./sql-error-mark";
  import { Decoration } from "@codemirror/view";
  import { currentStatementField, linesOf, setCurrentStatement } from "./sql-current-statement";
  import { shortcutStore } from "./shortcuts.svelte";
  import SnippetPicker from "./SnippetPicker.svelte";
  import SnippetVarsDialog from "./SnippetVarsDialog.svelte";
  import {
    autoContext,
    columnChoices,
    shortcutEntries,
    tableChoices,
    type SnippetContext,
  } from "./editor/snippet-env";
  import type { SnippetVariable } from "./snippet-vars";
  import {
    expandBinding,
    insertSnippet,
    snippetCompletions,
    snippetShortcutBindings,
    type AskVariables,
    type ExpandEnv,
  } from "./sql-snippet";
  import { snippets } from "./snippets.svelte";
  import {
    columnCompletion,
    hoverInfo,
    qualifiedNameAt,
    relationAt,
    tablesInScope,
  } from "./sql-complete";
  import { sqlFont } from "./editor.svelte";
  import { diagnosticsExtension, type DiagnosticsDeps } from "./editor/diagnostics";
  import { editorPrefs } from "./editor/editor-prefs.svelte";
  import { acceptGhost, dismissGhost, ghostSuggest } from "./editor/ghost-suggest";
  import { closeSignatureHelp, signatureHelp } from "./editor/signature-help";
  import { valuesCompletion } from "./editor/values-completion";
  import type { CheckResult, ColumnValues, RoutineSignature } from "./ipc";
  import type { ForeignKeyRef, SchemaRelation } from "./ipc";
  import { joinCompletion } from "./sql-join";
  import { OBJECT_MIME, parseDragged, type DraggedObject } from "./tree-object";
  import type { ErrorMark } from "./query.svelte";
  import type { EditorAction } from "./editor/actions";
  import { flashNotice, noticeExtension } from "./editor/notice";
  import { statementFolding } from "./editor/outline-fold";
  import { pasteListExtension } from "./editor/paste-list";
  import { rangeToRun } from "./editor/run-range";
  import { starTransaction } from "./editor/star";
  import { tablePeek } from "./editor/table-peek";
  import { transformTransaction } from "./editor/transform";
  import { cteRelations } from "./sql-complete";
  import RenameAliasDialog from "./RenameAliasDialog.svelte";
  import { aliasAt, renameAlias, type AliasAt } from "./sql-rename";
  import { pasteAsList, type TextTransform } from "./sql-text-tools";

  let {
    value = $bindable(""),
    schema = undefined,
    relations = [],
    foreignKeys = [],
    errorMark = null,
    readonly = false,
    initialSelection = null,
    initialTopPos = null,
    onrun,
    onrunScript,
    oncommit,
    onrollback,
    oncancel,
    onsave,
    onformat,
    onversions,
    onreveal,
    onopen,
    onobjectdrop,
    onroutine,
    onposition,
    onrunrange,
    onoutline,
    onpickobject,
    peekFor,
    statementAt,
    checkSql,
    canCheck,
    valuesFor,
    signaturesFor,
    suggestFor,
    snippetContext = null,
    onsavesnippet,
  }: {
    /**
     * Dónde se escribe: el servidor y la base deciden qué abreviaturas valen (su alcance) y
     * alimentan los valores automáticos (`${base}`, `${servidor}`, `${usuario}`). Sin esto solo
     * valen las abreviaturas no acotadas.
     */
    snippetContext?: SnippetContext | null;
    /** Guardar el texto como abreviatura nueva: abre el diálogo de abreviaturas con ese borrador. */
    onsavesnippet?: (body: string) => void;
    /**
     * La sentencia donde está el cursor, según el núcleo —quien decide dónde termina una—. La
     * revisión con el servidor, los avisos y el texto fantasma trabajan sobre ella.
     */
    statementAt?: (text: string, cursor: number) => Promise<{ text: string; offset: number } | null>;
    /** Revisa una sentencia con el servidor sin ejecutarla. Sin esto no hay errores subrayados. */
    checkSql?: (sql: string) => Promise<CheckResult>;
    /** La pestaña puede revisar ahora: no ejecuta, no hay transacción abierta y tiene sesión. */
    canCheck?: () => boolean;
    /** Valores de una columna (enum o frecuentes) para completar `estado = '`. */
    valuesFor?: (schema: string, table: string, column: string) => Promise<ColumnValues>;
    /** Sobrecargas de una función por nombre, para la ayuda de parámetros. */
    signaturesFor?: (schema: string | null, name: string) => Promise<RoutineSignature[]>;
    /** Sentencias ya ejecutadas que empiezan con este texto, para el fantasma del historial. */
    suggestFor?: (prefix: string) => Promise<string[]>;
    value?: string;
    schema?: SQLNamespace;
    /** Los mismos nombres en plano, para completar las columnas del `FROM` sin calificar. */
    relations?: SchemaRelation[];
    /** Para proponer el `ON` de un `JOIN`; sin ellas ese autocompletado no ofrece nada. */
    foreignKeys?: ForeignKeyRef[];
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
    /** Confirmar y revertir la transacción abierta de la pestaña. */
    oncommit?: () => void;
    onrollback?: () => void;
    oncancel?: () => void;
    /** Ctrl+S guarda donde ya se había guardado; Ctrl+Shift+S siempre pregunta. */
    onsave?: (askPath: boolean) => void;
    /** Ctrl+Mayús+F: la selección si hay una, o el documento entero. */
    onformat?: (selection: string, cursor: number) => void;
    /** Abrir las versiones anteriores del texto de esta pestaña. */
    onversions?: () => void;
    /** `Ctrl`+clic sobre una tabla del `FROM`/`JOIN`: la relación resuelta, para revelarla en el
     *  árbol. `null` cuando el clic no cayó sobre nada reconocible. */
    onreveal?: (relation: SchemaRelation | null) => void;
    /** `Ctrl`+clic sobre una tabla o vista: abrir sus datos. Sin esto se revela en el árbol. */
    onopen?: (relation: SchemaRelation) => void;
    /** `Mayús`+soltar un objeto del árbol: el texto a insertar (un `SELECT` con sus columnas). */
    onobjectdrop?: (object: DraggedObject) => Promise<string | null>;
    /** `Ctrl`+clic sobre una llamada a función: mostrar su definición. */
    onroutine?: (schema: string | null, name: string) => void;
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
    /**
     * «Ejecutar hasta el cursor» / «desde el cursor»: el tramo del script y dónde empieza (en
     * caracteres Unicode, para que la marca de un error caiga en su lugar). Pasa por el mismo
     * camino que el script entero, así que la confirmación de producción y las variables valen igual.
     */
    onrunrange?: (range: { text: string; base: number }) => void;
    /** Mostrar u ocultar el esquema del script. */
    onoutline?: () => void;
    /** Abrir el buscador para insertar un objeto en el cursor. */
    onpickobject?: () => void;
    /** Monta la tarjeta de vista rápida de una tabla dentro de `target`; devuelve cómo desmontarla. */
    peekFor?: (relation: SchemaRelation, target: HTMLElement) => () => void;
  } = $props();

  let element: HTMLDivElement;
  let view: EditorView | null = null;

  const language = new Compartment();

  // Las props se leen por función y no por valor: el editor se crea una sola vez y las ayudas tienen
  // que ver siempre la pestaña de ahora (si ejecuta, si abrió una transacción), no la de cuando nació.
  const diagnosticsDeps: DiagnosticsDeps = {
    statementAt: (text, cursor) => (statementAt ? statementAt(text, cursor) : Promise.resolve(null)),
    check: (sql) => (checkSql ? checkSql(sql) : Promise.resolve({ status: "skipped", reason: "sin servidor" })),
    canCheck: () => !readonly && (canCheck ? canCheck() : false),
    checkEnabled: () => editorPrefs.check && checkSql !== undefined,
    lintEnabled: () => editorPrefs.lint,
  };

  /**
   * La abreviatura con variables que espera sus valores. El diálogo vive acá y no en `QueryPanel`
   * porque quien lo pide es el editor (tabulador o lista de sugerencias) y hay un editor por
   * pestaña: así cada uno muestra el suyo.
   */
  let asking = $state<{
    title: string;
    description: string;
    variables: SnippetVariable[];
    tables: string[];
    columns: string[];
    done: (values: Record<string, string>) => void;
  } | null>(null);

  const askVariables: AskVariables = (item, variables, done) => {
    // Las opciones de catálogo se arman al preguntar y no antes: dependen de dónde está el cursor.
    const needsCatalog = variables.some((v) => v.kind === "table" || v.kind === "column");
    const text = view?.state.doc.toString() ?? "";
    const cursor = view?.state.selection.main.head ?? 0;
    asking = {
      title: `«${item.abbreviation}»`,
      description: item.description,
      variables,
      tables: needsCatalog ? tableChoices(relations) : [],
      columns: needsCatalog ? columnChoices(relations, text, cursor) : [],
      done,
    };
  };

  /** Las abreviaturas que valen en este servidor y esta base (ver `visibleFor`). */
  const visibleSnippets = () =>
    snippets.visibleFor(snippetContext?.profileId ?? null, snippetContext?.database ?? null);

  /**
   * Lo que la expansión necesita del lugar donde ocurre. Se lee por función en cada expansión y no
   * se copia al crear el editor: la base de la pestaña puede cambiar sin recrearlo.
   */
  const expandEnv: ExpandEnv = {
    auto: () =>
      autoContext(snippetContext ?? { profileId: null, database: null, server: null, user: null }),
    clipboard: () => navigator.clipboard.readText(),
    used: (id) => snippets.noteUsed(id),
  };

  /** El selector de abreviaturas con vista previa (`editor.snippetPicker`). */
  let pickingSnippet = $state(false);

  const theme = EditorView.theme({
    // El tamaño sale de la variable que maneja `sqlFont`: lo cambia el usuario y vale para todo el
    // SQL de la aplicación, no solo para este editor.
    "&": { height: "100%", fontSize: "var(--sql-font-size)", backgroundColor: "transparent" },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "var(--sql-leading, 1.65)" },
    ".cm-content": { paddingBlock: "8px" },
    // La sentencia que ejecutaría Ctrl+Enter: franja azul en el borde y un tinte apenas visible.
    ".cm-current-statement": {
      boxShadow: "inset 2px 0 0 var(--color-blue-500)",
      backgroundColor: "color-mix(in srgb, var(--color-blue-500) 6%, transparent)",
    },
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

  /** Un atajo reasignable del editor; sin combinación (desasignado) no genera ninguna entrada. */
  function bound(id: string, binding: Omit<KeyBinding, "key">): KeyBinding[] {
    const key = shortcutStore.combo(id);
    return key === null ? [] : [{ key, ...binding }];
  }

  // Va en un `Compartment` y se rearma cuando cambian los atajos (`shortcuts.svelte.ts`): el keymap
  // sigue en `Prec.highest` y cada entrada sigue devolviendo `false` cuando no le toca. Las que no
  // se reasignan (Tab, Escape, el zoom) se escriben a mano porque tienen más de un caso.
  const keys = new Compartment();

  function buildShortcuts() {
    return Prec.highest(
    keymap.of([
      // Primero de la lista, pero devuelve `false` cuando no le toca (ver `sql-snippet`): así el
      // tabulador sigue saltando entre los huecos de una expansión ya abierta.
      expandBinding(visibleSnippets, askVariables, expandEnv),
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
          // Orden de prioridad del tabulador: hueco de una expansión, sugerencia elegida de la
          // lista, texto fantasma del historial y, recién si no hay nada de eso, indentar.
          if (acceptGhost(target)) return true;
          return indentMore(target);
        },
        shift: (target) => {
          if (hasPrevSnippetField(target.state)) return false;
          return indentLess(target);
        },
      },
      ...bound("editor.run", {
        preventDefault: true,
        run: (target) => {
          const { from, to } = target.state.selection.main;
          const text = target.state.doc.toString();
          onrun?.(text.slice(from, to), toCharOffset(text, from));
          return true;
        },
      }),
      ...bound("editor.runScript", {
        preventDefault: true,
        run: () => {
          onrunScript?.();
          return true;
        },
      }),
      ...bound("editor.commit", {
        preventDefault: true,
        run: () => {
          oncommit?.();
          return true;
        },
      }),
      ...bound("editor.rollback", {
        preventDefault: true,
        run: () => {
          onrollback?.();
          return true;
        },
      }),
      {
        // Este keymap es `Prec.highest`, así que sin la guarda se comía el Escape que cierra el
        // autocompletado y el panel de búsqueda: cancelaba la consulta y la lista quedaba abierta.
        // Devolver `false` deja que lo atienda el manejador que corresponde.
        key: "Escape",
        run: (target) => {
          if (completionStatus(target.state) !== null) return false;
          if (searchPanelOpen(target.state)) return false;
          // Antes de cancelar la consulta, el Escape cierra lo que está flotando sobre el texto: la
          // ayuda de parámetros y el fantasma del historial. Cancelar es lo último.
          if (closeSignatureHelp(target)) return true;
          if (dismissGhost(target)) return true;
          oncancel?.();
          return true;
        },
      },
      ...bound("app.save", {
        preventDefault: true,
        run: () => {
          onsave?.(false);
          return true;
        },
      }),
      ...bound("app.saveAs", {
        preventDefault: true,
        run: () => {
          onsave?.(true);
          return true;
        },
      }),
      ...bound("editor.format", {
        preventDefault: true,
        run: (target) => {
          const { from, to } = target.state.selection.main;
          const text = target.state.doc.toString();
          onformat?.(text.slice(from, to), toCharOffset(text, from));
          return true;
        },
      }),
      ...bound("editor.versions", {
        preventDefault: true,
        run: () => {
          onversions?.();
          return true;
        },
      }),
      ...bound("editor.runToCursor", {
        preventDefault: true,
        run: () => (runRange("until"), true),
      }),
      ...bound("editor.runFromCursor", {
        preventDefault: true,
        run: () => (runRange("from"), true),
      }),
      ...bound("editor.expandStar", {
        preventDefault: true,
        run: () => (expandStarAtCursor(), true),
      }),
      ...bound("editor.rename", {
        preventDefault: true,
        run: () => (startRename(), true),
      }),
      ...bound("editor.outline", {
        preventDefault: true,
        run: () => (onoutline?.(), true),
      }),
      ...bound("editor.insertObject", {
        preventDefault: true,
        run: () => (onpickobject?.(), true),
      }),
      ...bound("editor.saveSnippet", {
        preventDefault: true,
        run: () => (void saveSnippetFromEditor(), true),
      }),
      ...bound("editor.snippetPicker", {
        preventDefault: true,
        run: () => ((pickingSnippet = true), true),
      }),
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
      // Al final y no al principio: los atajos de la aplicación y del editor ganan. El de una
      // abreviatura que coincide con alguno ya viene filtrado por `shortcutEntries`, así que acá
      // solo llegan los que no pisan nada.
      ...snippetShortcutBindings(
        shortcutEntries(visibleSnippets(), [...shortcutStore.bindings.values()]),
        askVariables,
        expandEnv,
      ),
    ]),
    );
  }

  // Se reasignó un atajo, o cambió una abreviatura o el lugar donde se escribe (que decide cuáles
  // valen): el editor abierto toma el keymap nuevo sin recrearse.
  $effect(() => {
    void shortcutStore.bindings;
    void snippets.items;
    void snippetContext?.profileId;
    void snippetContext?.database;
    view?.dispatch({ effects: keys.reconfigure(buildShortcuts()) });
  });

  /**
   * `Ctrl`+clic sobre una tabla del `FROM`/`JOIN` la revela en el árbol.
   *
   * Solo intercepta el clic cuando de verdad resuelve una tabla: devolver `false` en cualquier otro
   * caso deja que el clic mueva el cursor como siempre, que es lo que se espera si el `Ctrl`+clic no
   * cayó sobre nada reconocible.
   */
  const revealHandler = EditorView.domEventHandlers({
    /**
     * Soltar un objeto del árbol. Sin modificador lo resuelve el editor solo —inserta el nombre,
     * que viaja como texto—; con `Mayús` inserta un `SELECT` con todas sus columnas, que hay que ir
     * a buscar al servidor y por eso no puede ser el comportamiento de fábrica.
     */
    drop(event, editorView) {
      const dragged = parseDragged(event.dataTransfer?.getData(OBJECT_MIME));
      if (!dragged || !event.shiftKey || !onobjectdrop) return false;

      event.preventDefault();
      const at = editorView.posAtCoords({ x: event.clientX, y: event.clientY }) ?? editorView.state.selection.main.head;
      void onobjectdrop(dragged).then((text) => {
        if (!text) return;
        editorView.dispatch({ changes: { from: at, insert: text }, selection: { anchor: at + text.length } });
        editorView.focus();
      });
      return true;
    },

    mousedown(event, editorView) {
      if (!(event.ctrlKey || event.metaKey)) return false;

      const pos = editorView.posAtCoords({ x: event.clientX, y: event.clientY });
      if (pos === null) return false;

      const text = editorView.state.doc.toString();
      const qualified = qualifiedNameAt(text, pos);
      if (!qualified) return false;

      // Una CTE no es una tabla aunque se llame como una: abrir los datos de la real sería mentir.
      const isCte =
        qualified.schema === null &&
        cteRelations(text, pos, relations).some((cte) => cte.name === qualified.name);
      const found = isCte ? null : relationAt(relations, tablesInScope(text, pos), qualified);
      if (found) {
        // `Ctrl`+clic abre la tabla; con `Mayús` la revela en el árbol, que era lo único que hacía.
        if (event.shiftKey) onreveal?.(found);
        else (onopen ?? onreveal)?.(found);
        return true;
      }

      // Un nombre seguido de `(` que no es una tabla es una llamada: se busca la función.
      if (/^\s*\(/.test(text.slice(qualified.to)) && onroutine) {
        onroutine(qualified.schema, qualified.name);
        return true;
      }
      return false;
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
          keys.of(untrack(() => buildShortcuts())),
          basicSetup,
          language.of(sqlExtension(untrack(() => schema))),
          // Ayudas al escribir. Cada una se prende y apaga desde Preferencias y calla sola si falta
          // lo que necesita (sesión, servidor, nada que decir): son ayudas, no carteles.
          diagnosticsExtension(diagnosticsDeps),
          signatureHelp({
            signatures: (schemaName, name) => (signaturesFor ? signaturesFor(schemaName, name) : Promise.resolve([])),
            enabled: () => signaturesFor !== undefined,
          }),
          ghostSuggest({
            statementAt: (text, cursor) => (statementAt ? statementAt(text, cursor) : Promise.resolve(null)),
            suggest: (prefix) => (suggestFor ? suggestFor(prefix) : Promise.resolve([])),
            enabled: () => editorPrefs.ghost && suggestFor !== undefined,
          }),
          errorMarkField,
          currentStatementField,
          revealHandler,
          columnHover,
          // Tarjeta de la tabla al pasar el cursor: hover aparte del de columnas, cada uno responde
          // solo a lo suyo. Sin `peekFor` no hay quién la dibuje.
          ...(peekFor
            ? [tablePeek({ relations: () => relations, mount: (relation, target) => peekFor!(relation, target) })]
            : []),
          noticeExtension,
          pasteListExtension(),
          statementFolding,
          syntaxHighlighting(sqlHighlight),
          sqlNesting,
          theme,
          // La lista de autocompletar y los tooltips cuelgan del `body`: adentro del editor los
          // recorta el `overflow-hidden` del panel, y con el cursor en las últimas líneas —pegado a
          // la grilla de abajo— la lista quedaba cortada o directamente invisible.
          tooltips({ parent: document.body, position: "fixed" }),
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
      // El `ON` de un `JOIN`, a partir de las claves foráneas. La tabla sin esquema se busca en la
      // lista plana: gana `public` si hay dos con el mismo nombre, que es lo que resuelve el servidor
      // con el `search_path` de fábrica.
      PostgreSQL.language.data.of({
        autocomplete: joinCompletion(
          () => foreignKeys,
          (ref) =>
            ref.schema ??
            (relations.find((r) => r.name === ref.name && r.schema === "public") ??
              relations.find((r) => r.name === ref.name))?.schema ??
            null,
        ),
      }),
      // Y las abreviaturas, que si no habría que recordar de memoria.
      PostgreSQL.language.data.of({ autocomplete: snippetCompletions(visibleSnippets, askVariables, expandEnv) }),
      // Los valores de la columna que se está comparando (`estado = '|`). Lee las relaciones y el
      // pedido por función, igual que el resto, para que cambiar de base no obligue a reconfigurar.
      PostgreSQL.language.data.of({
        autocomplete: valuesCompletion({
          relations: () => relations,
          fetch: (schemaName, table, column) =>
            valuesFor ? valuesFor(schemaName, table, column) : Promise.resolve({ kind: "none", values: [] }),
          enabled: () => valuesFor !== undefined,
        }),
      }),
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

  /**
   * Marca la sentencia que se ejecutaría ahora, o la saca con `null`. `offset` viene en caracteres
   * del núcleo y `length` en unidades del editor (el largo del texto de la sentencia).
   */
  export function markStatement(statement: { offset: number; text: string } | null) {
    if (!view) return;
    const doc = view.state.doc;
    const text = doc.toString();
    if (!statement || statement.text.trim() === text.trim()) {
      // Con una sola sentencia en todo el editor, marcarla sería teñir el editor entero.
      view.dispatch({ effects: setCurrentStatement.of(Decoration.none) });
      return;
    }
    const from = toEditorIndex(text, statement.offset);
    const to = Math.min(doc.length, from + statement.text.length);
    view.dispatch({ effects: setCurrentStatement.of(linesOf(doc, from, to)) });
  }

  /** Lleva el cursor a un desplazamiento en caracteres y lo deja a la vista, centrado. */
  export function goTo(chars: number) {
    if (!view) return;
    const at = Math.min(toEditorIndex(view.state.doc.toString(), chars), view.state.doc.length);
    view.dispatch({
      selection: { anchor: at },
      effects: EditorView.scrollIntoView(at, { y: "center" }),
    });
    view.focus();
  }

  export function focus() {
    view?.focus();
  }

  /** Lleva el cursor a una posición del documento (UTF-16) y la deja a la vista: lo usa el esquema. */
  export function goToIndex(index: number) {
    if (!view) return;
    const at = Math.min(index, view.state.doc.length);
    view.dispatch({
      selection: { anchor: at },
      effects: EditorView.scrollIntoView(at, { y: "start", yMargin: 24 }),
    });
    view.focus();
  }

  /** Inserta `text` donde está el cursor (reemplazando la selección) y deja el cursor al final. */
  export function insertText(text: string) {
    if (!view) return;
    view.dispatch(
      view.state.update(view.state.replaceSelection(text), {
        userEvent: "input.complete",
        scrollIntoView: true,
      }),
    );
    view.focus();
  }

  /** Corre «hasta» o «desde» el cursor; sin nada que correr, avisa junto al cursor. */
  function runRange(kind: "until" | "from") {
    if (!view) return;
    const text = view.state.doc.toString();
    const range = rangeToRun(text, view.state.selection.main.head, kind);
    if (!range) {
      flashNotice(view, kind === "until" ? t("No hay ninguna sentencia hasta acá.") : t("No hay ninguna sentencia desde acá."));
      return;
    }
    onrunrange?.(range);
  }

  function expandStarAtCursor() {
    if (!view) return;
    const spec = starTransaction(view.state, relations);
    if (!spec) {
      flashNotice(view, t("No hay un * para expandir acá, o falta conocer alguna tabla del FROM."));
      return;
    }
    view.dispatch(view.state.update(spec));
  }

  /**
   * Guardar como abreviatura lo seleccionado o, sin selección, la sentencia donde está el cursor
   * (la que decide el núcleo: es la misma que marca la franja azul y que ejecutaría Ctrl+Enter).
   */
  async function saveSnippetFromEditor() {
    if (!view) return;
    const { from, to, empty, head } = view.state.selection.main;
    const text = view.state.doc.toString();
    let body = empty ? "" : text.slice(from, to);
    if (body.trim() === "" && statementAt) {
      const found = await statementAt(text, toCharOffset(text, head)).catch(() => null);
      body = found?.text ?? "";
    }
    if (body.trim() === "") {
      if (view) flashNotice(view, t("No hay nada que guardar: selecciona SQL o ponte sobre una sentencia."));
      return;
    }
    onsavesnippet?.(body);
  }

  /** Elegida una abreviatura en el selector: se inserta donde está el cursor, como con el tabulador. */
  function insertPickedSnippet(item: (typeof snippets.items)[number]) {
    pickingSnippet = false;
    if (!view) return;
    const { from, to, empty } = view.state.selection.main;
    insertSnippet(view, item, from, to, askVariables, expandEnv, empty ? undefined : view.state.sliceDoc(from, to));
    view.focus();
  }

  function applyTransform(kind: TextTransform) {
    if (!view) return;
    const spec = transformTransaction(view.state, kind);
    if (!spec) {
      flashNotice(view, t("El texto ya está así: no hay nada que cambiar."));
      return;
    }
    view.dispatch(view.state.update(spec));
    view.focus();
  }

  /**
   * Pegar el portapapeles como lista desde la paleta. Leer el portapapeles puede pedir permiso o
   * fallar según el sistema: si no se puede, se dice, y queda `Ctrl+Mayús+V`, que no lo necesita.
   */
  async function pasteListNow() {
    if (!view) return;
    try {
      const list = pasteAsList(await navigator.clipboard.readText());
      if (list === null) {
        flashNotice(view, t("Lo copiado no es una columna: pega normal con Ctrl+V."));
        return;
      }
      view.dispatch(
        view.state.update(view.state.replaceSelection(list), { userEvent: "input.paste", scrollIntoView: true }),
      );
      view.focus();
    } catch {
      flashNotice(view, t("No se pudo leer el portapapeles: usa Ctrl+Mayús+V."));
    }
  }

  /** El alias que se está por renombrar; el diálogo vive acá porque hay un editor por pestaña. */
  let renaming = $state<{ info: AliasAt; pos: number; text: string } | null>(null);

  function startRename() {
    if (!view) return;
    const pos = view.state.selection.main.head;
    const text = view.state.doc.toString();
    const info = aliasAt(text, pos);
    if (!info) {
      flashNotice(view, t("No hay un alias ni una CTE bajo el cursor."));
      return;
    }
    renaming = { info, pos, text };
  }

  function applyRename(newName: string) {
    const current = renaming;
    renaming = null;
    if (!view || !current) return;
    // Todas las apariciones en una sola transacción: un `Ctrl+Z` las deshace juntas. Si el texto
    // cambió mientras el diálogo estaba abierto, las posiciones ya no valen y no se toca nada.
    if (view.state.doc.toString() !== current.text) {
      flashNotice(view, t("El texto cambió mientras tanto: vuelve a pedir el renombrado."));
      return;
    }
    view.dispatch(
      view.state.update({
        changes: renameAlias(current.info.occurrences, newName),
        userEvent: "input",
      }),
    );
    view.focus();
  }

  /** Lo que la paleta le puede pedir a este editor (ver `editor/actions.ts`). */
  export function runAction(action: EditorAction) {
    if (action === "expandStar") expandStarAtCursor();
    else if (action === "renameAlias") startRename();
    else if (action === "runToCursor") runRange("until");
    else if (action === "runFromCursor") runRange("from");
    else if (action === "pasteList") void pasteListNow();
    else if (action === "saveSnippet") void saveSnippetFromEditor();
    else if (action === "snippetPicker") pickingSnippet = true;
    else if (action.startsWith("text:")) applyTransform(action.slice(5) as TextTransform);
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

{#if renaming}
  <RenameAliasDialog
    text={renaming.text}
    pos={renaming.pos}
    info={renaming.info}
    onapply={applyRename}
    onclose={() => {
      renaming = null;
      view?.focus();
    }}
  />
{/if}

{#if pickingSnippet}
  <SnippetPicker
    items={visibleSnippets()}
    usage={snippets.usage}
    onpick={insertPickedSnippet}
    onclose={() => {
      pickingSnippet = false;
      view?.focus();
    }}
  />
{/if}

{#if asking}
  <SnippetVarsDialog
    title={asking.title}
    description={asking.description}
    variables={asking.variables}
    tables={asking.tables}
    columns={asking.columns}
    onsubmit={asking.done}
    onclose={() => {
      asking = null;
      view?.focus();
    }}
  />
{/if}
