<script lang="ts">
  import { syntaxHighlighting } from "@codemirror/language";
  import { PostgreSQL, sql } from "@codemirror/lang-sql";
  import { EditorState } from "@codemirror/state";
  import { EditorView } from "@codemirror/view";
  import { untrack } from "svelte";
  import { sqlHighlight } from "./sql-highlight";
  import { sqlNesting } from "./sql-nested";
  import { sqlFont } from "./editor.svelte";

  /**
   * SQL de solo lectura, coloreado.
   *
   * Reusa el mismo resaltado que el editor ([`sqlHighlight`] y [`sqlNesting`]) en vez de repintar a
   * mano, así el DDL que la aplicación muestra tiene exactamente los colores del editor —incluido el
   * cuerpo de una función, que es SQL adentro de una cadena `$$`—. Sin la decoración del editor
   * (números de línea, línea activa, búsqueda): es un `<pre>` con color.
   */
  let { code }: { code: string } = $props();

  let element: HTMLDivElement;
  let view: EditorView | null = null;

  const theme = EditorView.theme({
    // Mismo tamaño que el editor de consultas: leer un DDL y escribirlo son la misma lectura.
    "&": {
      backgroundColor: "transparent",
      color: "inherit",
      fontSize: "var(--sql-font-size)",
    },
    ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.5" },
    ".cm-content": { padding: "0" },
    ".cm-line": { padding: "0" },
  });

  $effect(() => {
    view = new EditorView({
      parent: element,
      state: EditorState.create({
        doc: untrack(() => code),
        extensions: [
          sql({ dialect: PostgreSQL }),
          syntaxHighlighting(sqlHighlight),
          sqlNesting,
          theme,
          EditorView.lineWrapping,
          // No editable: sin cursor ni foco, pero el texto se sigue pudiendo seleccionar y copiar.
          EditorView.editable.of(false),
          EditorState.readOnly.of(true),
        ],
      }),
    });

    return () => {
      view?.destroy();
      view = null;
    };
  });

  // El DDL cambia al elegir otro objeto: se repone el documento en vez de recrear el editor.
  $effect(() => {
    if (view && code !== untrack(() => view!.state.doc.toString())) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: code } });
    }
  });

  // Cambiar la letra sin avisarle deja el ancho de carácter viejo, y con él el salto de línea.
  $effect(() => {
    sqlFont.size;
    view?.requestMeasure();
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={element}
  class="select-text [&_.cm-editor]:bg-transparent"
  onwheel={(event) => {
    if (!event.ctrlKey) return;
    event.preventDefault();
    if (event.deltaY < 0) sqlFont.bigger();
    else sqlFont.smaller();
  }}
></div>
