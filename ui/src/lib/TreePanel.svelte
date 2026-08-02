<script lang="ts">
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { lookOf } from "./badges";
  import { explorer, visibleRows, type Row } from "./explorer.svelte";

  let {
    onconnect,
    onnew,
  }: {
    onconnect: (profileId: string) => void;
    /** Abre el diálogo de servidor nuevo desde el estado vacío. */
    onnew?: () => void;
  } = $props();

  /**
   * Todas las filas miden lo mismo, así que la ventana visible se calcula con una división en vez
   * de medir cada fila. Un esquema con miles de tablas dibuja solo lo que entra en pantalla.
   */
  const ROW_HEIGHT = 28;
  const OVERSCAN = 8;

  let scrollTop = $state(0);
  let viewportHeight = $state(600);
  let viewport = $state<HTMLDivElement | null>(null);

  const rows = $derived(visibleRows(explorer.roots, explorer.search));
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const visible = $derived(
    rows.slice(start, start + Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2),
  );

  const needle = $derived(explorer.search.trim().toLowerCase());

  function activate(row: Row) {
    explorer.select(row);
    // Un contenedor se abre con el mismo clic que lo selecciona: pedir doble clic para algo tan
    // frecuente es una regla que hay que descubrir.
    if (row.hasChildren && !explorer.needsConnection(row)) {
      explorer.toggle(row);
    }
  }

  /** Deja la fila `index` dentro de la ventana visible, sin moverla si ya se ve. */
  function reveal(index: number) {
    if (!viewport) return;
    const top = index * ROW_HEIGHT;
    if (top < scrollTop) viewport.scrollTop = top;
    else if (top + ROW_HEIGHT > scrollTop + viewportHeight) {
      viewport.scrollTop = top + ROW_HEIGHT - viewportHeight;
    }
  }

  function move(delta: number) {
    if (rows.length === 0) return;
    const current = rows.findIndex((row) => row.key === explorer.selected?.key);
    const next = Math.min(rows.length - 1, Math.max(0, current < 0 ? 0 : current + delta));
    explorer.select(rows[next]);
    reveal(next);
  }

  function goTo(index: number) {
    if (rows.length === 0) return;
    explorer.select(rows[index]);
    reveal(index);
  }

  /**
   * El teclado se maneja en el contenedor y no en cada fila: con la ventana deslizante, las filas
   * de arriba y de abajo ni siquiera existen en el DOM, así que no hay adónde mover el foco. El
   * árbol entero es un punto de tabulación y la fila elegida se anuncia con `aria-activedescendant`.
   */
  function onKey(event: KeyboardEvent) {
    const row = explorer.selected;

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        move(-1);
        break;
      case "Home":
        event.preventDefault();
        goTo(0);
        break;
      case "End":
        event.preventDefault();
        goTo(rows.length - 1);
        break;
      case "ArrowRight":
        if (!row) return;
        event.preventDefault();
        // Abrir; si ya estaba abierto, bajar al primer hijo, que es lo que acaba de aparecer.
        if (row.hasChildren && !row.expanded) explorer.toggle(row);
        else if (row.expanded) move(1);
        break;
      case "ArrowLeft":
        if (!row) return;
        event.preventDefault();
        // Cerrar; si ya estaba cerrado, subir al padre, que es de donde cuelga.
        if (row.expanded) explorer.toggle(row);
        else move(-1);
        break;
      case "Enter":
      case " ":
        if (!row) return;
        event.preventDefault();
        activate(row);
        break;
    }
  }

  /**
   * Parte la etiqueta en los tramos que coinciden con la búsqueda y los que no. Sin esto hay que
   * releer cada fila para encontrar por qué apareció en el resultado.
   */
  function pieces(label: string): { text: string; hit: boolean }[] {
    if (needle === "") return [{ text: label, hit: false }];

    const out: { text: string; hit: boolean }[] = [];
    const lower = label.toLowerCase();
    let at = 0;
    for (;;) {
      const found = lower.indexOf(needle, at);
      if (found < 0) break;
      if (found > at) out.push({ text: label.slice(at, found), hit: false });
      out.push({ text: label.slice(found, found + needle.length), hit: true });
      at = found + needle.length;
    }
    if (at < label.length) out.push({ text: label.slice(at), hit: false });
    return out;
  }
</script>

<div
  bind:this={viewport}
  class="h-full overflow-auto outline-none"
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  onkeydown={onKey}
  bind:clientHeight={viewportHeight}
  role="tree"
  tabindex="0"
  aria-label="Servidores y objetos"
  aria-activedescendant={explorer.selected ? `tree-${explorer.selected.key}` : undefined}
>
  {#if explorer.roots.length === 0}
    <Empty
      icon="server"
      title="Todavía no hay servidores"
      hint="Agregá una conexión para empezar a explorar sus bases, esquemas y tablas."
    >
      {#if onnew}
        <button class="btn btn-primary" onclick={onnew}>
          <Icon name="plus" size={12} />
          Nuevo servidor
        </button>
      {/if}
    </Empty>
  {:else if rows.length === 0}
    <Empty
      icon="search"
      title="Sin coincidencias"
      hint="La búsqueda solo alcanza a lo que ya se cargó del árbol: abrí los nodos donde puede estar «{explorer.search}»."
    />
  {:else}
    {#if needle !== ""}
      <p class="px-3 py-1.5 text-[11px] muted">
        {rows.length}
        {rows.length === 1 ? "coincidencia" : "coincidencias"} entre lo ya cargado
      </p>
    {/if}

    <div class="relative" style="height: {rows.length * ROW_HEIGHT}px">
      {#each visible as row, index (start + index)}
        {@const look = lookOf(row.node?.kind ?? null)}
        {@const isServer = row.node === null}
        {@const isSelected = explorer.selected?.key === row.key}
        <!-- El teclado lo maneja el contenedor: las filas fuera de la ventana no están en el DOM. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="tree-{row.key}"
          class="group absolute left-0 flex w-full items-center gap-1.5 rounded-md pr-1 text-sm
                 {isSelected
            ? 'bg-blue-50 text-blue-900 dark:bg-blue-950/60 dark:text-blue-100'
            : 'hover:bg-zinc-100 dark:hover:bg-zinc-800/70'}"
          style="top: {(start + index) * ROW_HEIGHT}px; height: {ROW_HEIGHT}px; padding-left: {4 +
            row.level * 13}px"
          role="treeitem"
          tabindex="-1"
          aria-expanded={row.hasChildren ? row.expanded : undefined}
          aria-selected={isSelected}
          onclick={() => activate(row)}
        >
          <!--
            Guías de indentación: en un árbol de cinco niveles, saber de qué esquema cuelga una
            tabla no debería requerir contar sangrías con el dedo.
          -->
          {#each { length: row.level }, depth (depth)}
            <span
              class="pointer-events-none absolute inset-y-0 w-px bg-zinc-200 dark:bg-zinc-800"
              style="left: {10 + depth * 13}px"
            ></span>
          {/each}

          {#if isSelected}
            <span
              class="pointer-events-none absolute inset-y-0 left-0 w-0.5 rounded-r bg-blue-600
                     dark:bg-blue-400"
            ></span>
          {/if}

          <button
            class="relative grid size-4 shrink-0 place-items-center rounded text-zinc-400
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
            <span
              class="dot {row.connected ? 'dot-on' : 'dot-off'}"
              title={row.connected ? "Conectado" : "Sin conectar"}
            ></span>
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
            {#each pieces(row.label) as piece, position (position)}
              {#if piece.hit}
                <mark class="rounded bg-amber-200 text-inherit dark:bg-amber-500/40"
                  >{piece.text}</mark
                >
              {:else}{piece.text}{/if}
            {/each}
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

          <span class="ml-auto flex shrink-0 items-center gap-0.5 pl-1">
            {#if row.children !== null && row.hasChildren}
              <button
                class="btn btn-ghost btn-icon size-6 opacity-0 focus-visible:opacity-100
                       group-hover:opacity-100"
                title="Volver a leer este nodo del servidor"
                aria-label="Recargar"
                tabindex="-1"
                onclick={(event) => {
                  event.stopPropagation();
                  explorer.reload(row);
                }}
              >
                <Icon name="refresh" size={11} />
              </button>
            {/if}

            {#if isServer && !row.connected}
              <button
                class="btn btn-ghost px-2 py-0.5 text-xs text-blue-600 hover:bg-blue-50
                       dark:text-blue-400 dark:hover:bg-blue-950"
                tabindex="-1"
                onclick={(event) => {
                  event.stopPropagation();
                  onconnect(row.profileId);
                }}
              >
                Conectar
              </button>
            {/if}
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>
