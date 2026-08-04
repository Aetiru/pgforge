<script lang="ts">
  import { syntaxHighlighting } from "@codemirror/language";
  import { PostgreSQL, sql } from "@codemirror/lang-sql";
  import { EditorState } from "@codemirror/state";
  import { EditorView } from "@codemirror/view";
  import { untrack } from "svelte";
  import { sqlHighlight } from "./sql-highlight";

  /**
   * SQL de solo lectura, coloreado.
   *
   * Reusa el mismo resaltado que el editor ([`sqlHighlight`]) en vez de repintar a mano, así el DDL
   * que la aplicación muestra tiene exactamente los colores del editor —incluidas las cadenas
   * delimitadas con `$$` de los cuerpos de función, que un tokenizador propio erraría—. Sin la
   * decoración del editor (números de línea, línea activa, búsqueda): es un `<pre>` con color.
   */
  let { code }: { code: string } = $props();

  let element: HTMLDivElement;
  let view: EditorView | null = null;

  const theme = EditorView.theme({
    "&": { backgroundColor: "transparent", color: "inherit" },
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
</script>

<div bind:this={element} class="select-text text-xs [&_.cm-editor]:bg-transparent"></div>
