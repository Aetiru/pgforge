<script lang="ts">
  import { badgeFor } from "./badges";
  import { explorer, visibleRows, type Row } from "./explorer.svelte";

  /**
   * Todas las filas miden lo mismo, así que la ventana visible se calcula con una división en vez
   * de medir cada fila. Un esquema con miles de tablas dibuja solo lo que entra en pantalla.
   */
  const ROW_HEIGHT = 26;
  const OVERSCAN = 8;

  let scrollTop = $state(0);
  let viewportHeight = $state(600);

  const rows = $derived(visibleRows(explorer.roots));
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const visible = $derived(
    rows.slice(start, start + Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2),
  );

  function onScroll(event: Event) {
    scrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }

  function onKey(event: KeyboardEvent, row: Row) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      explorer.select(row);
    } else if (event.key === "ArrowRight" && !row.expanded) {
      explorer.toggle(row);
    } else if (event.key === "ArrowLeft" && row.expanded) {
      explorer.toggle(row);
    }
  }
</script>

<div
  class="h-full overflow-auto"
  onscroll={onScroll}
  bind:clientHeight={viewportHeight}
  role="tree"
  tabindex="-1"
>
  {#if rows.length === 0}
    <p class="p-4 text-sm text-neutral-500">
      No hay servidores conectados. Agregá uno para empezar.
    </p>
  {:else}
    <div class="relative" style="height: {rows.length * ROW_HEIGHT}px">
      {#each visible as row, index (row.key)}
        {@const badge = badgeFor(row.node?.kind ?? null)}
        <div
          class="absolute left-0 flex w-full items-center gap-1.5 pr-2 text-sm
                 hover:bg-neutral-100 dark:hover:bg-neutral-800
                 {explorer.selected?.key === row.key
            ? 'bg-blue-100 dark:bg-blue-950'
            : ''}"
          style="top: {(start + index) * ROW_HEIGHT}px; height: {ROW_HEIGHT}px; padding-left: {6 +
            row.level * 14}px"
          role="treeitem"
          tabindex="0"
          aria-expanded={row.hasChildren ? row.expanded : undefined}
          aria-selected={explorer.selected?.key === row.key}
          onclick={() => explorer.select(row)}
          ondblclick={() => explorer.toggle(row)}
          onkeydown={(event) => onKey(event, row)}
        >
          <button
            class="w-4 shrink-0 text-xs text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
            onclick={(event) => {
              event.stopPropagation();
              explorer.toggle(row);
            }}
            aria-label={row.expanded ? "Contraer" : "Expandir"}
            disabled={!row.hasChildren}
          >
            {#if row.loading}
              ⋯
            {:else if row.hasChildren}
              {row.expanded ? "▾" : "▸"}
            {/if}
          </button>

          {#if badge.text}
            <span
              class="shrink-0 rounded px-1 py-px font-mono text-[10px] leading-4 {badge.tone}"
            >
              {badge.text}
            </span>
          {/if}

          <span class="truncate" title={row.comment ?? row.label}>{row.label}</span>

          {#if row.detail}
            <span class="truncate text-xs text-neutral-400 dark:text-neutral-500">
              {row.detail}
            </span>
          {/if}

          {#if row.error}
            <span class="truncate text-xs text-red-600 dark:text-red-400" title={row.error}>
              {row.error}
            </span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
