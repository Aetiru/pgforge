<script lang="ts">
  import { untrack } from "svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { outline, type OutlineItem } from "./sql-outline";

  /**
   * El esquema del script: una fila por sentencia, para saltar a cualquiera sin recorrer cien líneas.
   * Se arma con el partidor de **navegación** de `sql-outline.ts`; el que decide qué se ejecuta sigue
   * siendo el del núcleo, así que esta lista puede diferir en un caso raro sin que cambie lo que corre.
   *
   * Se recalcula con un respiro tras cada cambio: partir el script en cada tecla no aporta nada que
   * se vea. `line` es la línea del cursor y marca la sentencia donde se está escribiendo.
   */
  let {
    text,
    line,
    onpick,
    onclose,
  }: {
    text: string;
    /** Línea del cursor, desde 1. */
    line: number;
    /** Desplazamiento del documento (UTF-16) donde empieza la sentencia elegida. */
    onpick: (from: number) => void;
    onclose: () => void;
  } = $props();

  interface Row {
    item: OutlineItem;
    endLine: number;
  }

  function rowsOf(source: string): Row[] {
    return outline(source).map((item) => {
      let endLine = item.line;
      for (let at = item.from; at < item.to; at += 1) if (source.charCodeAt(at) === 10) endLine += 1;
      return { item, endLine };
    });
  }

  let rows = $state.raw<Row[]>(untrack(() => rowsOf(text)));
  let filter = $state("");

  $effect(() => {
    const source = text;
    const timer = setTimeout(() => (rows = rowsOf(source)), 250);
    return () => clearTimeout(timer);
  });

  const shown = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    if (needle === "") return rows;
    return rows.filter(
      ({ item }) => item.title.toLowerCase().includes(needle) || item.kind.toLowerCase().includes(needle),
    );
  });

  /** La sentencia que contiene la línea del cursor; entre dos, la anterior. */
  const current = $derived.by(() => {
    const inside = rows.find((row) => line >= row.item.line && line <= row.endLine);
    if (inside) return inside.item.from;
    return [...rows].reverse().find((row) => row.item.line <= line)?.item.from ?? -1;
  });
</script>

<aside
  class="flex w-60 shrink-0 flex-col border-l border-zinc-200 bg-white dark:border-zinc-700 dark:bg-zinc-900"
  aria-label="Esquema del script"
>
  <div class="divider-b flex items-center gap-1.5 px-2 py-1.5">
    <span class="text-xs font-semibold">Esquema</span>
    <span class="tag tag-neutral font-mono">{rows.length}</span>
    <button
      class="btn btn-ghost btn-icon ml-auto"
      aria-label="Cerrar el esquema del script"
      title="Cerrar el esquema del script"
      onclick={onclose}
    >
      <Icon name="close" size={12} />
    </button>
  </div>

  <div class="px-2 py-1.5">
    <input
      class="field w-full text-xs"
      placeholder="Filtrar sentencias…"
      aria-label="Filtrar las sentencias del script"
      bind:value={filter}
    />
  </div>

  <div class="min-h-0 flex-1 overflow-auto pb-1">
    {#if shown.length === 0}
      <Empty
        icon="sql"
        title={rows.length === 0 ? "El script está vacío" : "Ninguna sentencia coincide"}
      />
    {:else}
      {#each shown as row (row.item.from)}
        <button
          class="flex w-full flex-col items-start gap-0.5 border-l-2 px-2 py-1 text-left text-xs
                 {row.item.from === current
            ? 'border-blue-500 bg-blue-500/10'
            : 'border-transparent hover:bg-zinc-100 dark:hover:bg-zinc-800'}"
          title={row.item.title}
          onclick={() => onpick(row.item.from)}
        >
          <span class="flex w-full items-center gap-1.5">
            <span class="tabular-nums muted">{row.item.line}</span>
            <span class="tag tag-neutral truncate">{row.item.kind}</span>
          </span>
          <span class="w-full truncate font-mono">{row.item.title}</span>
        </button>
      {/each}
    {/if}
  </div>
</aside>
