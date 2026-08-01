<script lang="ts">
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import Icon from "./Icon.svelte";
  import type { DataTab, Row } from "./data.svelte";

  let { tab }: { tab: DataTab } = $props();

  /** Lo que se muestra en lugar de un NULL, que no es lo mismo que una celda vacía. */
  const NULL = "[null]";

  const SAMPLE = 50;
  const CHAR_WIDTH = 7.3;
  const MIN_WIDTH = 72;
  const MAX_WIDTH = 340;

  const shape = $derived(tab.shape);

  const definitions = $derived.by<Column<Row>[]>(() => {
    if (!shape) return [];

    const sample = tab.rows.slice(0, SAMPLE);

    const cells: Column<Row>[] = shape.columns.map((column, index) => {
      const longest = sample.reduce(
        (max, row) => Math.max(max, (tab.value(row, index) ?? NULL).length),
        column.name.length,
      );

      return {
        key: `${index}-${column.name}`,
        // El tipo va en el encabezado: al editar es lo que dice qué se puede escribir en la celda.
        header: `${column.name}  ${column.typeName}`,
        width: Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(longest * CHAR_WIDTH) + 24)),
        value: (row) => oneLine(tab.value(row, index)),
        edit: (row) => tab.value(row, index),
        title: (row) => tab.value(row, index) ?? undefined,
        tone: (row) => {
          if (tab.value(row, index) === null) return "italic text-zinc-400 dark:text-zinc-600";
          return row.edited.has(index) ? "font-medium text-amber-700 dark:text-amber-400" : "";
        },
      };
    });

    return [
      {
        key: "#",
        header: "",
        width: 44,
        align: "right",
        value: (row) => marca(row),
      },
      ...cells,
    ];
  });

  /** Un símbolo por fila dice más que una columna de números en una grilla que se está editando. */
  function marca(row: Row): string {
    if (row.deleted) return "✕";
    if (row.isNew) return "+";
    if (row.edited.size > 0) return "•";
    return String(row.index + 1);
  }

  function oneLine(value: string | null): string {
    return value === null ? NULL : value.replace(/\s*\n\s*/g, " ↵ ");
  }

  /** La clave de la columna arranca con su posición; la del marcador no es un número. */
  function columnIndex(column: Column<Row>): number {
    return Number(column.key.split("-")[0]);
  }

  function editable(row: Row, column: Column<Row>): boolean {
    if (!tab.editable || row.deleted) return false;

    const definition = shape?.columns[columnIndex(column)];
    // Las generadas y las de identidad las calcula el servidor: ofrecer editarlas sería mentir.
    return definition !== undefined && !definition.generated;
  }

  /** La fila sobre la que actúan los botones de la barra. */
  let selected = $state<number | null>(null);
  const selectedRow = $derived(tab.rows.find((row) => row.index === selected) ?? null);
</script>

<div class="flex h-full flex-col">
  <header class="divider-b flex flex-wrap items-center gap-1.5 px-2 py-1.5">
    <button class="btn" disabled={tab.loading || tab.saving} onclick={() => tab.load()}>
      <Icon name="refresh" size={12} />
      Refrescar
    </button>

    {#if tab.editable}
      <span class="mx-1 h-4 w-px bg-zinc-200 dark:bg-zinc-800"></span>

      <button class="btn" disabled={tab.saving} onclick={() => tab.addRow()}>
        <Icon name="plus" size={12} />
        Fila
      </button>
      <button
        class="btn"
        disabled={selectedRow === null || tab.saving}
        title="Marca la fila elegida para borrarla al guardar"
        onclick={() => selectedRow && tab.toggleDelete(selectedRow)}
      >
        {selectedRow?.deleted ? "No borrar" : "Borrar fila"}
      </button>
      <button
        class="btn"
        disabled={tab.pending === 0 || tab.saving}
        title="Muestra el SQL exacto que se va a ejecutar"
        onclick={() => tab.showPreview()}
      >
        Ver el SQL
      </button>
      <button class="btn" disabled={tab.pending === 0 || tab.saving} onclick={() => tab.discard()}>
        Descartar
      </button>
      <button
        class="btn btn-primary"
        disabled={tab.pending === 0 || tab.saving}
        onclick={() => tab.save()}
      >
        Guardar{tab.pending > 0 ? ` (${tab.pending})` : ""}
      </button>
    {/if}

    <span class="ml-auto flex items-center gap-2 text-xs muted">
      {#if tab.loading || tab.saving}
        <span class="spinner"></span>
      {/if}
      <span>
        {tab.rows.length}
        {tab.rows.length === 1 ? "fila" : "filas"}{tab.complete ? "" : "…"}
      </span>
      {#if shape?.key}
        <span title="Con esta clave se identifica cada fila">
          {shape.key.kind === "primary" ? "clave primaria" : "índice único"}: {shape.key.columns.join(", ")}
        </span>
      {/if}
    </span>
  </header>

  {#if shape?.readOnly}
    <div
      class="flex items-start gap-2 border-b border-amber-200 bg-amber-50 px-3 py-2 text-sm
             text-amber-800 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-300"
    >
      <span class="mt-0.5">⚠</span>
      <span>{shape.readOnly}</span>
    </div>
  {/if}

  {#if tab.error}
    <div
      class="flex items-start gap-2 border-b border-rose-200 bg-rose-50 px-3 py-2 text-sm
             text-rose-700 dark:border-rose-900 dark:bg-rose-950 dark:text-rose-300"
    >
      <span class="flex-1">{tab.error}</span>
      <button class="btn btn-ghost px-1.5 py-0.5" onclick={() => (tab.error = null)}>
        <Icon name="close" size={12} />
      </button>
    </div>
  {/if}

  <div class="min-h-0 flex-1">
    {#if !shape}
      <p class="p-4 text-sm muted">Abriendo la tabla…</p>
    {:else}
      <DataGrid
        columns={definitions}
        rows={tab.rows}
        rowKey={(row) => row.index}
        selectedKey={selected}
        empty="La tabla está vacía."
        editable={tab.editable ? editable : undefined}
        onedit={(row, column, value) => tab.edit(row, columnIndex(column), value)}
        onnearend={() => tab.more()}
        rowClass={(row) =>
          row.deleted
            ? "line-through opacity-50"
            : row.isNew
              ? "bg-emerald-50/60 dark:bg-emerald-950/30"
              : ""}
        onselect={(row) => (selected = row.index)}
      />
    {/if}
  </div>

  {#if tab.editable && tab.rows.length > 0}
    <p class="divider-t px-3 py-1 text-xs muted">
      Doble clic en una celda para editarla · ∅ pone NULL · nada se escribe hasta que se guarda
    </p>
  {/if}
</div>

{#if tab.preview}
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
    <div class="card flex max-h-[80vh] w-full max-w-3xl flex-col p-5 shadow-xl">
      <h2 class="text-base font-medium">Esto es lo que se va a ejecutar</h2>
      <p class="mt-1 text-xs muted">
        Las {tab.preview.length}
        {tab.preview.length === 1 ? "sentencia va" : "sentencias van"} en una sola transacción: si alguna
        falla, no se aplica ninguna.
      </p>

      <div class="mt-3 min-h-0 flex-1 overflow-auto">
        {#each tab.preview as statement, index (index)}
          <div class="mb-2 rounded border border-zinc-200 p-2 dark:border-zinc-800">
            <pre class="font-mono text-xs whitespace-pre-wrap select-text">{statement.sql}</pre>
            {#if statement.params.length > 0}
              <div class="mt-1 flex flex-wrap gap-1.5 text-[11px] muted">
                {#each statement.params as param, position (position)}
                  <span class="rounded bg-zinc-100 px-1.5 py-px font-mono dark:bg-zinc-800">
                    ${position + 1} = {param === null ? "NULL" : param}
                  </span>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>

      <div class="mt-4 flex justify-end gap-2">
        <button class="btn" onclick={() => (tab.preview = null)}>Cerrar</button>
        <button class="btn btn-primary" disabled={tab.saving} onclick={() => tab.save()}>
          Guardar
        </button>
      </div>
    </div>
  </div>
{/if}
