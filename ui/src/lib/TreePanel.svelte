<script lang="ts">
  import Icon from "./Icon.svelte";
  import { lookOf } from "./badges";
  import { explorer, visibleRows, type Row } from "./explorer.svelte";

  let { onconnect }: { onconnect: (profileId: string) => void } = $props();

  /**
   * Todas las filas miden lo mismo, así que la ventana visible se calcula con una división en vez
   * de medir cada fila. Un esquema con miles de tablas dibuja solo lo que entra en pantalla.
   */
  const ROW_HEIGHT = 28;
  const OVERSCAN = 8;

  let scrollTop = $state(0);
  let viewportHeight = $state(600);

  const rows = $derived(visibleRows(explorer.roots, explorer.search));
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const visible = $derived(
    rows.slice(start, start + Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2),
  );

  function activate(row: Row) {
    explorer.select(row);
    // Un contenedor se abre con el mismo clic que lo selecciona: pedir doble clic para algo tan
    // frecuente es una regla que hay que descubrir.
    if (row.hasChildren && !explorer.needsConnection(row)) {
      explorer.toggle(row);
    }
  }

  function onKey(event: KeyboardEvent, row: Row) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate(row);
    } else if (event.key === "ArrowRight" && !row.expanded) {
      explorer.toggle(row);
    } else if (event.key === "ArrowLeft" && row.expanded) {
      explorer.toggle(row);
    }
  }
</script>

<div
  class="h-full overflow-auto"
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  bind:clientHeight={viewportHeight}
  role="tree"
  tabindex="-1"
>
  {#if explorer.roots.length === 0}
    <div class="px-4 py-6 text-center">
      <p class="text-sm muted">Todavía no hay servidores.</p>
      <p class="mt-1 text-xs muted">Agregá uno con el botón + de arriba.</p>
    </div>
  {:else if rows.length === 0}
    <p class="px-4 py-6 text-center text-sm muted">
      Nada coincide con «{explorer.search}» entre lo que ya se cargó.
    </p>
  {:else}
    <div class="relative" style="height: {rows.length * ROW_HEIGHT}px">
      {#each visible as row, index (start + index)}
        {@const look = lookOf(row.node?.kind ?? null)}
        {@const isServer = row.node === null}
        {@const isSelected = explorer.selected?.key === row.key}
        <div
          class="group absolute left-0 flex w-full items-center gap-1.5 rounded-md pr-2 text-sm
                 {isSelected
            ? 'bg-blue-50 text-blue-900 dark:bg-blue-950/60 dark:text-blue-100'
            : 'hover:bg-zinc-100 dark:hover:bg-zinc-800/70'}"
          style="top: {(start + index) * ROW_HEIGHT}px; height: {ROW_HEIGHT}px; padding-left: {4 +
            row.level * 13}px"
          role="treeitem"
          tabindex="0"
          aria-expanded={row.hasChildren ? row.expanded : undefined}
          aria-selected={isSelected}
          onclick={() => activate(row)}
          onkeydown={(event) => onKey(event, row)}
        >
          <button
            class="grid size-4 shrink-0 place-items-center rounded text-zinc-400
                   hover:text-zinc-900 dark:hover:text-zinc-100"
            onclick={(event) => {
              event.stopPropagation();
              explorer.toggle(row);
            }}
            aria-label={row.expanded ? "Contraer" : "Expandir"}
            tabindex="-1"
            disabled={!row.hasChildren || explorer.needsConnection(row)}
          >
            {#if row.loading}
              <span class="spinner"></span>
            {:else if row.hasChildren && !explorer.needsConnection(row)}
              <Icon
                name="chevron"
                size={12}
                class="transition-transform {row.expanded ? 'rotate-90' : ''}"
              />
            {/if}
          </button>

          {#if isServer}
            <span class="dot {row.connected ? 'dot-on' : 'dot-off'}"></span>
          {/if}

          <Icon name={look.icon} class={look.tone} />

          <!--
            El nombre es lo que identifica la fila, así que cede espacio último. El detalle se
            achica primero: sin esto, un host largo dejaba el nombre del servidor en dos letras.
          -->
          <span
            class="min-w-0 truncate {isServer ? 'font-medium' : ''}"
            title={row.comment ?? row.label}
          >
            {row.label}
          </span>

          {#if row.detail}
            <span class="min-w-0 shrink-[100] truncate text-xs muted" title={row.detail}>
              {row.detail}
            </span>
          {/if}

          {#if row.error}
            <span
              class="min-w-0 shrink-[100] truncate text-xs text-rose-600 dark:text-rose-400"
              title={row.error}
            >
              {row.error}
            </span>
          {/if}

          {#if isServer && !row.connected}
            <button
              class="btn btn-ghost ml-auto shrink-0 px-2 py-0.5 text-xs
                     text-blue-600 hover:bg-blue-50 dark:text-blue-400 dark:hover:bg-blue-950"
              onclick={(event) => {
                event.stopPropagation();
                onconnect(row.profileId);
              }}
            >
              Conectar
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
