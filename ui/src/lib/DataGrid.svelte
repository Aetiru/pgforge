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
    /**
     * Con qué se ordena la columna. Sin esto se ordena por el texto que se muestra, que alcanza
     * para nombres pero no para números formateados: «1.234» y «999» no se comparan como texto.
     */
    sort?: (row: T) => string | number;
  }
</script>

<script lang="ts" generics="T">
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";

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
    sortable = false,
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
    /**
     * Ordenar al hacer clic en el encabezado. Lo piden las grillas que muestran una foto completa
     * —las estadísticas del monitor, el resultado de una consulta—, no las que se paginan o se
     * editan, donde el número de fila tiene que seguir significando su posición real.
     */
    sortable?: boolean;
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
  const MIN_COLUMN = 48;

  let scrollTop = $state(0);
  let viewportHeight = $state(400);
  let viewport = $state<HTMLDivElement | null>(null);

  /** Qué celda se está editando: la fila por su clave y la columna por la suya. */
  let editing = $state<{ row: string | number; column: string } | null>(null);
  let draft = $state("");
  /** `true` cuando el editor está por escribir un NULL en vez de un texto. */
  let draftNull = $state(false);

  /** Anchos cambiados a mano. Lo que no está acá conserva el de la definición. */
  let widths = $state<Record<string, number>>({});
  let sort = $state<{ key: string; descending: boolean } | null>(null);

  function widthOf(column: Column<T>): number {
    return widths[column.key] ?? column.width;
  }

  const ordered = $derived.by(() => {
    if (!sort) return rows;
    const column = columns.find((item) => item.key === sort!.key);
    if (!column) return rows;

    const value = (row: T) => (column.sort ? column.sort(row) : column.value(row));
    const direction = sort.descending ? -1 : 1;

    return [...rows].sort((left, right) => {
      const a = value(left);
      const b = value(right);
      if (typeof a === "number" && typeof b === "number") return (a - b) * direction;
      return String(a).localeCompare(String(b), "es", { numeric: true }) * direction;
    });
  });

  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const visible = $derived(
    ordered.slice(start, start + Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2),
  );
  const totalWidth = $derived(columns.reduce((sum, column) => sum + widthOf(column), 0));

  function onScroll(event: Event & { currentTarget: HTMLDivElement }) {
    scrollTop = event.currentTarget.scrollTop;

    const lastVisible = Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT);
    if (onnearend && ordered.length - lastVisible < NEAR_END) {
      onnearend();
    }
  }

  function toggleSort(column: Column<T>) {
    if (!sortable) return;
    // Ascendente, descendente y sin orden: el tercer clic devuelve la grilla a como llegó.
    if (sort?.key !== column.key) sort = { key: column.key, descending: false };
    else if (!sort.descending) sort = { key: column.key, descending: true };
    else sort = null;
  }

  function startResize(event: MouseEvent, column: Column<T>) {
    event.preventDefault();
    event.stopPropagation();
    const origin = event.clientX;
    const initial = widthOf(column);

    const move = (moved: MouseEvent) => {
      widths[column.key] = Math.max(MIN_COLUMN, initial + moved.clientX - origin);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("cursor-col-resize");
    };
    document.body.classList.add("cursor-col-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  /** Deja la fila `index` a la vista; la usa la navegación con flechas. */
  function reveal(index: number) {
    if (!viewport) return;
    const top = index * ROW_HEIGHT;
    if (top < scrollTop) viewport.scrollTop = top;
    else if (top + ROW_HEIGHT > scrollTop + viewportHeight) {
      viewport.scrollTop = top + ROW_HEIGHT - viewportHeight;
    }
  }

  function moveSelection(delta: number) {
    if (!onselect || ordered.length === 0) return;
    const current = ordered.findIndex((row) => rowKey(row) === selectedKey);
    const next = Math.min(ordered.length - 1, Math.max(0, current < 0 ? 0 : current + delta));
    onselect(ordered[next]);
    reveal(next);
  }

  function onGridKey(event: KeyboardEvent) {
    // Con una celda abierta las flechas son del editor de texto, no de la grilla.
    if (editing || !onselect) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveSelection(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveSelection(-1);
    } else if (event.key === "Home") {
      event.preventDefault();
      moveSelection(-ordered.length);
    } else if (event.key === "End") {
      event.preventDefault();
      moveSelection(ordered.length);
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

<div
  bind:this={viewport}
  class="h-full overflow-auto outline-none"
  onscroll={onScroll}
  onkeydown={onGridKey}
  bind:clientHeight={viewportHeight}
  role="grid"
  tabindex={onselect ? 0 : -1}
>
  <div style="width: {totalWidth}px">
    <div class="panel divider-b sticky top-0 z-10 flex text-xs font-medium muted">
      {#each columns as column (column.key)}
        {@const active = sort?.key === column.key}
        <div
          class="group relative shrink-0 {column.align === 'right' ? 'text-right' : ''}"
          style="width: {widthOf(column)}px"
        >
          <button
            class="w-full truncate px-2 py-1 text-left {column.align === 'right'
              ? 'text-right'
              : ''} {sortable ? 'hover:text-zinc-900 dark:hover:text-zinc-100' : 'cursor-default'}
              {active ? 'text-zinc-900 dark:text-zinc-100' : ''}"
            title={sortable ? `${column.header} — clic para ordenar` : column.header}
            tabindex="-1"
            onclick={() => toggleSort(column)}
          >
            {column.header}
            {#if active}
              <Icon
                name="chevron"
                size={9}
                class="ml-0.5 inline-block {sort?.descending ? '-rotate-90' : 'rotate-90'}"
              />
            {/if}
          </button>

          <!--
            El tirador del ancho: dos píxeles a la vista, seis para el mouse. Es lo primero que se
            busca cuando una columna corta justo el valor que hay que leer.
          -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute inset-y-0 -right-[3px] z-10 w-[6px] cursor-col-resize
                   hover:bg-blue-500/40"
            onmousedown={(event) => startResize(event, column)}
            ondblclick={() => delete widths[column.key]}
          ></div>
        </div>
      {/each}
    </div>

    {#if ordered.length > 0}
      <div class="relative" style="height: {ordered.length * ROW_HEIGHT}px">
        <!--
          La clave es la posición absoluta de la fila, no su identidad. En una lista con ventana
          deslizante los nodos se reciclan por posición de todos modos, y una clave derivada de los
          datos obliga a que sean únicos: pg_stat_statements, por ejemplo, distingue sus filas por
          (usuario, base, queryid) y repite el texto de la consulta, lo que hacía abortar el render.
        -->
        {#each visible as row, index (start + index)}
          {@const key = rowKey(row)}
          {@const selected = selectedKey === key}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="absolute left-0 flex text-sm
                   {selected
              ? 'bg-blue-100/70 text-blue-950 dark:bg-blue-950/70 dark:text-blue-100'
              : (start + index) % 2 === 1
                ? 'bg-zinc-50/60 hover:bg-zinc-100 dark:bg-zinc-900/40 dark:hover:bg-zinc-800/70'
                : 'hover:bg-zinc-100 dark:hover:bg-zinc-800/70'}
                   {rowClass?.(row) ?? ''}"
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
                  style="width: {widthOf(column)}px; height: {ROW_HEIGHT}px"
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
                  style="width: {widthOf(column)}px"
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

  <!--
    El vacío queda fuera del contenedor de ancho fijo y pegado a la izquierda: si estuviera dentro,
    en una grilla de veinte columnas el mensaje aparecería a dos mil píxeles de donde se está
    mirando.
  -->
  {#if ordered.length === 0}
    <div class="sticky left-0 flex w-full justify-center py-6">
      <Empty icon="table" title={empty} />
    </div>
  {/if}
</div>
