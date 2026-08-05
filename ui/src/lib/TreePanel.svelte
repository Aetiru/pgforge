<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { environmentOf, isReadOnly } from "./access.svelte";
  import { envLook, GROUP_LOOK, lookOf, READ_ONLY_LOOK, tagLook } from "./badges";
  import { explorer, visibleRows, type Row } from "./explorer.svelte";
  import { describeError, folderOf } from "./ipc";

  let {
    onconnect,
    onnew,
    ongroup,
  }: {
    onconnect: (profileId: string) => void;
    /** Abre el diálogo de servidor nuevo desde el estado vacío. */
    onnew?: () => void;
    /** Abre el diálogo para renombrar o deshacer una carpeta de conexiones. */
    ongroup?: (name: string) => void;
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

  /** El servidor que se está arrastrando y la carpeta sobre la que caería si se soltara ahora. */
  let dragging = $state<string | null>(null);
  let dropGroup = $state<string | null | undefined>(undefined);
  let moveError = $state<string | null>(null);

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
        // Sobre un servidor desconectado no hay nada que expandir: lo que se espera es conectarlo.
        if (explorer.needsConnection(row)) onconnect(row.profileId);
        else activate(row);
        break;
    }
  }

  /**
   * A qué carpeta va a parar un servidor soltado sobre esta fila. Vale cualquier fila de la
   * carpeta y no solo su encabezado: apuntar a una línea de 28 píxeles con el mouse apretado es
   * más difícil de lo que parece.
   */
  function dropTargetOf(row: Row): string | null {
    return row.kind === "group" ? row.group! : (row.group ?? null);
  }

  function onDrop(group: string | null) {
    const profileId = dragging;
    dragging = null;
    dropGroup = undefined;
    if (!profileId) return;
    // Mover un servidor reescribe el archivo de conexiones: si no se pudo, hay que decirlo, o la
    // fila vuelve a su lugar sin explicación.
    explorer.moveToGroup(profileId, group).catch((error) => (moveError = describeError(error)));
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

<!-- El contenedor recibe lo que se suelta fuera de toda carpeta: ahí es donde el servidor queda
     suelto. Las filas que sí tienen carpeta cortan la propagación antes de llegar acá. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={viewport}
  class="h-full overflow-auto outline-none {dragging !== null && dropGroup === null
    ? 'rounded-md ring-1 ring-blue-500/50'
    : ''}"
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  onkeydown={onKey}
  ondragover={(event) => {
    if (dragging === null) return;
    event.preventDefault();
    dropGroup = null;
  }}
  ondrop={(event) => {
    event.preventDefault();
    onDrop(null);
  }}
  bind:clientHeight={viewportHeight}
  role="tree"
  tabindex="0"
  aria-label="Servidores y objetos"
  aria-activedescendant={explorer.selected ? `tree-${explorer.selected.key}` : undefined}
>
  {#if moveError}
    <Alert tone="bad" onclose={() => (moveError = null)}>{moveError}</Alert>
  {/if}

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
        {@const isGroup = row.kind === "group"}
        {@const isServer = row.kind === "server"}
        {@const look = isGroup ? GROUP_LOOK : lookOf(row.node?.kind ?? null)}
        {@const isFolder = row.node !== null && folderOf(row.node.kind) !== null}
        {@const isSelected = explorer.selected?.key === row.key}
        {@const isDropTarget = dragging !== null && dropGroup === dropTargetOf(row)}
        <!-- El teclado lo maneja el contenedor: las filas fuera de la ventana no están en el DOM. -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          id="tree-{row.key}"
          class="group absolute left-0 flex w-full items-center gap-1.5 rounded-md pr-1 text-sm
                 {isSelected
            ? 'bg-blue-50 text-blue-900 dark:bg-blue-950/60 dark:text-blue-100'
            : 'hover:bg-zinc-100 dark:hover:bg-zinc-800/70'}
                 {isDropTarget && dropGroup !== null ? 'bg-blue-100/70 dark:bg-blue-900/40' : ''}
                 {dragging === row.profileId ? 'opacity-40' : ''}"
          style="top: {(start + index) * ROW_HEIGHT}px; height: {ROW_HEIGHT}px; padding-left: {4 +
            row.level * 13}px"
          role="treeitem"
          tabindex="-1"
          aria-expanded={row.hasChildren ? row.expanded : undefined}
          aria-selected={isSelected}
          onclick={() => activate(row)}
          ondblclick={() => {
            // Doble clic sobre un servidor apagado: lo que se quiere es entrar.
            if (explorer.needsConnection(row)) onconnect(row.profileId);
          }}
          draggable={isServer}
          ondragstart={(event) => {
            dragging = row.profileId;
            event.dataTransfer?.setData("text/plain", row.label);
            if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
          }}
          ondragend={() => {
            dragging = null;
            dropGroup = undefined;
          }}
          ondragover={(event) => {
            if (dragging === null) return;
            event.preventDefault();
            event.stopPropagation();
            dropGroup = dropTargetOf(row);
          }}
          ondrop={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onDrop(dropTargetOf(row));
          }}
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

          {#if isServer && isReadOnly(row.profileId)}
            <span class="shrink-0 muted" title={READ_ONLY_LOOK.title}>
              <Icon name={READ_ONLY_LOOK.icon} size={11} />
            </span>
          {/if}

          <!--
            El nombre es lo que identifica la fila, así que cede espacio último. El detalle se
            achica primero: sin esto, un host largo dejaba el nombre del servidor en dos letras.
          -->
          <span
            class="min-w-0 truncate {isServer || isGroup ? 'font-medium' : ''}"
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

          <!--
            Los rasgos del objeto van como etiquetas y no como texto: que un rol pueda entrar o que
            una tabla filtre filas se reconoce por el color, sin leer la fila entera.
          -->
          {#each row.node?.tags ?? [] as tag (tag)}
            {@const badge = tagLook(tag)}
            <span class="tag {badge.tone} shrink-0" title={badge.title}>{badge.label}</span>
          {/each}

          <!-- El entorno del servidor se ve en la misma fila que su nombre: es donde el usuario
               elige a qué se conecta, y es el único momento en que puede elegir otro. -->
          {#if isServer && environmentOf(row.profileId)}
            {@const badge = envLook(environmentOf(row.profileId)!)}
            <span class="tag {badge.tone} shrink-0" title={badge.title}>{badge.label}</span>
          {/if}

          {#if row.detail && !isFolder}
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
            {#if isGroup && ongroup}
              <button
                class="btn btn-ghost btn-icon size-6 opacity-0 focus-visible:opacity-100
                       group-hover:opacity-100"
                title="Renombrar o deshacer la carpeta"
                aria-label="Editar la carpeta"
                tabindex="-1"
                onclick={(event) => {
                  event.stopPropagation();
                  ongroup(row.group!);
                }}
              >
                <Icon name="edit" size={11} />
              </button>
            {/if}

            {#if !isGroup && row.children !== null && row.hasChildren}
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

            <!-- El contador de una carpeta va al final y alineado: leerlo en columna es lo que
                 permite comparar de un vistazo qué esquema tiene qué. -->
            {#if isFolder && row.detail}
              <span class="seg-count tabular-nums">{row.detail}</span>
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
