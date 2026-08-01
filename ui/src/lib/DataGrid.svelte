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
  }: {
    columns: Column<T>[];
    rows: T[];
    rowKey: (row: T) => string | number;
    selectedKey?: string | number | null;
    onselect?: (row: T) => void;
    empty?: string;
  } = $props();

  /**
   * Misma técnica que el árbol: filas de altura fija y ventana calculada por división. El
   * dashboard se refresca cada dos segundos, así que dibujar mil filas en cada muestra sería el
   * mayor costo de toda la aplicación.
   */
  const ROW_HEIGHT = 24;
  const OVERSCAN = 10;

  let scrollTop = $state(0);
  let viewportHeight = $state(400);

  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const visible = $derived(
    rows.slice(start, start + Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2),
  );
  const totalWidth = $derived(columns.reduce((sum, column) => sum + column.width, 0));
</script>

<div
  class="h-full overflow-auto"
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  bind:clientHeight={viewportHeight}
>
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
              <div
                class="shrink-0 truncate px-2 py-0.5 {column.align === 'right'
                  ? 'text-right tabular-nums'
                  : ''} {column.tone?.(row) ?? ''}"
                style="width: {column.width}px"
                title={column.title?.(row)}
              >
                {column.value(row)}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
