<script lang="ts" module>
  export interface Column<T> {
    key: string;
    header: string;
    /** Ancho en píxeles. La grilla no mide el contenido: el ancho es parte de la definición. */
    width: number;
    align?: "left" | "right";
    value: (row: T) => string;
    /** Texto del tooltip, para lo que no entra en la celda. */
    title?: (row: T) => string | undefined;
    /** Clases extra según el valor, para resaltar filas problemáticas. */
    tone?: (row: T) => string | undefined;
    /** El texto con el que se abre el editor, que puede no ser el que se muestra. */
    edit?: (row: T) => string | null;
  }
</script>

<script lang="ts" generics="T">
  let {
    columns,
    rows,
    rowKey,
    selectedKey = null,
    onselect,
    empty = "Sin datos.",
    editable,
    onedit,
    onnearend,
    rowClass,
  }: {
    columns: Column<T>[];
    rows: T[];
    rowKey: (row: T) => string | number;
    selectedKey?: string | number | null;
    onselect?: (row: T) => void;
    empty?: string;
    /** Sin esto la grilla es de solo lectura, que es como la usan el dashboard y el editor. */
    editable?: (row: T, column: Column<T>) => boolean;
    onedit?: (row: T, column: Column<T>, value: string | null) => void;
    /** Se llama al acercarse al final de lo cargado, para pedir la página siguiente. */
    onnearend?: () => void;
    rowClass?: (row: T) => string | undefined;
  } = $props();

  /**
   * Misma técnica que el árbol: filas de altura fija y ventana calculada por división. El
   * dashboard se refresca cada dos segundos, así que dibujar mil filas en cada muestra sería el
   * mayor costo de toda la aplicación.
   */
  const ROW_HEIGHT = 24;
  const OVERSCAN = 10;
  /** Cuántas filas antes del final disparan la carga de la página siguiente. */
  const NEAR_END = 40;

  let scrollTop = $state(0);
  let viewportHeight = $state(400);

  /** Qué celda se está editando: la fila por su clave y la columna por la suya. */
  let editing = $state<{ row: string | number; column: string } | null>(null);
  let draft = $state("");
  /** `true` cuando el editor está por escribir un NULL en vez de un texto. */
  let draftNull = $state(false);

  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const visible = $derived(
    rows.slice(start, start + Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2),
  );
  const totalWidth = $derived(columns.reduce((sum, column) => sum + column.width, 0));

  function onScroll(event: Event & { currentTarget: HTMLDivElement }) {
    scrollTop = event.currentTarget.scrollTop;

    const lastVisible = Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT);
    if (onnearend && rows.length - lastVisible < NEAR_END) {
      onnearend();
    }
  }

  function open(row: T, column: Column<T>) {
    if (!editable?.(row, column)) return;

    const value = column.edit ? column.edit(row) : column.value(row);
    editing = { row: rowKey(row), column: column.key };
    draftNull = value === null;
    draft = value ?? "";
  }

  function commit(row: T, column: Column<T>) {
    if (!editing) return;
    onedit?.(row, column, draftNull ? null : draft);
    editing = null;
  }

  function onKey(event: KeyboardEvent, row: T, column: Column<T>) {
    if (event.key === "Enter") {
      event.preventDefault();
      commit(row, column);
    } else if (event.key === "Escape") {
      event.preventDefault();
      editing = null;
    }
  }
</script>

<div class="h-full overflow-auto" onscroll={onScroll} bind:clientHeight={viewportHeight}>
  <div style="width: {totalWidth}px">
    <div class="panel divider-b sticky top-0 z-10 flex text-xs font-medium muted">
      {#each columns as column (column.key)}
        <div
          class="shrink-0 truncate px-2 py-1 {column.align === 'right' ? 'text-right' : ''}"
          style="width: {column.width}px"
        >
          {column.header}
        </div>
      {/each}
    </div>

    {#if rows.length === 0}
      <p class="p-4 text-sm muted">{empty}</p>
    {:else}
      <div class="relative" style="height: {rows.length * ROW_HEIGHT}px">
        <!--
          La clave es la posición absoluta de la fila, no su identidad. En una lista con ventana
          deslizante los nodos se reciclan por posición de todos modos, y una clave derivada de los
          datos obliga a que sean únicos: pg_stat_statements, por ejemplo, distingue sus filas por
          (usuario, base, queryid) y repite el texto de la consulta, lo que hacía abortar el render.
        -->
        {#each visible as row, index (start + index)}
          {@const key = rowKey(row)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="absolute left-0 flex text-sm hover:bg-zinc-100 dark:hover:bg-zinc-800/70
                   {rowClass?.(row) ?? ''}
                   {selectedKey === key
              ? 'bg-blue-50 text-blue-900 dark:bg-blue-950/60 dark:text-blue-100'
              : ''}"
            style="top: {(start + index) *
              ROW_HEIGHT}px; height: {ROW_HEIGHT}px; width: {totalWidth}px"
            role="row"
            tabindex="-1"
            onclick={() => onselect?.(row)}
          >
            {#each columns as column (column.key)}
              {#if editing && editing.row === key && editing.column === column.key}
                <!-- svelte-ignore a11y_autofocus -->
                <div
                  class="flex shrink-0 items-center gap-1 bg-white px-1 ring-2 ring-blue-500
                         dark:bg-zinc-900"
                  style="width: {column.width}px; height: {ROW_HEIGHT}px"
                >
                  <input
                    class="w-full min-w-0 bg-transparent text-sm outline-none
                           {draftNull ? 'italic text-zinc-400' : ''}"
                    value={draftNull ? "[null]" : draft}
                    autofocus
                    oninput={(event) => {
                      draft = event.currentTarget.value;
                      draftNull = false;
                    }}
                    onkeydown={(event) => onKey(event, row, column)}
                    onblur={() => commit(row, column)}
                  />
                  <!--
                    Sin un botón, un NULL sería imposible de escribir: cualquier texto que uno teclee
                    es una cadena, y la vacía no es lo mismo que la ausencia de valor.
                  -->
                  <button
                    class="shrink-0 rounded px-1 text-[10px] text-zinc-400 hover:bg-zinc-200
                           hover:text-zinc-700 dark:hover:bg-zinc-700 dark:hover:text-zinc-100"
                    title="Poner NULL"
                    tabindex="-1"
                    onmousedown={(event) => {
                      // `mousedown` y no `click`: el `blur` del input confirmaría antes.
                      event.preventDefault();
                      draftNull = true;
                      draft = "";
                    }}
                  >
                    ∅
                  </button>
                </div>
              {:else}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="shrink-0 truncate px-2 py-0.5 {column.align === 'right'
                    ? 'text-right tabular-nums'
                    : ''} {column.tone?.(row) ?? ''}
                    {editable?.(row, column) ? 'cursor-text' : ''}"
                  style="width: {column.width}px"
                  title={column.title?.(row)}
                  ondblclick={() => open(row, column)}
                >
                  {column.value(row)}
                </div>
              {/if}
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
