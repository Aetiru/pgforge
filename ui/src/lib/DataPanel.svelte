<script lang="ts">
  import { readOnlyReason } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import type { DataTab, Row } from "./data.svelte";

  let { tab }: { tab: DataTab } = $props();

  /** Lo que se muestra en lugar de un NULL, que no es lo mismo que una celda vacía. */
  const NULL = "[null]";

  const SAMPLE = 50;
  const CHAR_WIDTH = 7.3;
  const MIN_WIDTH = 72;
  const MAX_WIDTH = 340;

  const shape = $derived(tab.shape);
  const readOnlyProfile = $derived(readOnlyReason(tab.profileId));

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
        tone: (row) => {
          if (row.deleted) return "text-rose-600 dark:text-rose-400";
          if (row.isNew) return "text-emerald-600 dark:text-emerald-400";
          if (row.edited.size > 0) return "text-amber-600 dark:text-amber-400";
          return "text-zinc-400 dark:text-zinc-600";
        },
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
  <header class="toolbar">
    <button class="btn" disabled={tab.loading || tab.saving} onclick={() => tab.load()}>
      <Icon name="refresh" size={12} />
      Refrescar
    </button>

    {#if tab.editable}
      <span class="toolbar-sep"></span>

      <button class="btn" disabled={tab.saving} onclick={() => tab.addRow()}>
        <Icon name="plus" size={12} />
        Fila
      </button>
      <button
        class="btn {selectedRow?.deleted ? '' : 'btn-danger-ghost'}"
        disabled={selectedRow === null || tab.saving}
        title="Marca la fila elegida para borrarla al guardar"
        onclick={() => selectedRow && tab.toggleDelete(selectedRow)}
      >
        <Icon name="trash" size={12} />
        {selectedRow?.deleted ? "No borrar" : "Borrar fila"}
      </button>

      <span class="toolbar-sep"></span>

      <button
        class="btn"
        disabled={tab.pending === 0 || tab.saving}
        title="Muestra el SQL exacto que se va a ejecutar"
        onclick={() => tab.showPreview()}
      >
        <Icon name="sql" size={12} />
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
        {#if tab.saving}<span class="spinner"></span>{/if}
        Guardar
        {#if tab.pending > 0}
          <span class="rounded bg-white/20 px-1 text-[11px] tabular-nums">{tab.pending}</span>
        {/if}
      </button>
    {/if}

    <span class="ml-auto flex items-center gap-2 text-xs muted">
      {#if tab.loading}
        <span class="spinner"></span>
      {/if}
      <span class="tabular-nums">
        {tab.rows.length}
        {tab.rows.length === 1 ? "fila" : "filas"}{tab.complete ? "" : " o más"}
      </span>
      {#if shape?.key}
        <span class="tag tag-neutral" title="Con esta clave se identifica cada fila al guardar">
          <Icon name="key" size={10} />
          {shape.key.columns.join(", ")}
        </span>
      {/if}
    </span>
  </header>

  <!-- Dos motivos distintos por los que no se puede editar; el del perfil manda porque vale para
       cualquier tabla que se abra. -->
  {#if readOnlyProfile}
    <Alert tone="warn">{readOnlyProfile}</Alert>
  {:else if shape?.readOnly}
    <Alert tone="warn">{shape.readOnly}</Alert>
  {/if}

  {#if tab.error}
    <Alert tone="bad" onclose={() => (tab.error = null)}>{tab.error}</Alert>
  {/if}

  <div class="min-h-0 flex-1">
    {#if !shape}
      <p class="flex items-center gap-2 p-4 text-sm muted">
        <span class="spinner"></span>
        Abriendo la tabla…
      </p>
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
    <p class="divider-t flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-1 text-xs muted">
      <span><span class="kbd">Doble clic</span> en una celda para editarla</span>
      <span><span class="kbd">∅</span> pone NULL</span>
      <span><span class="kbd">Enter</span> confirma · <span class="kbd">Esc</span> descarta</span>
      <span class="ml-auto">Nada se escribe en la base hasta que se guarda.</span>
    </p>
  {/if}
</div>

{#if tab.preview}
  <Modal
    title="Esto es lo que se va a ejecutar"
    subtitle="Las {tab.preview.length} {tab.preview.length === 1
      ? 'sentencia va'
      : 'sentencias van'} en una sola transacción: si alguna falla, no se aplica ninguna."
    size="xl"
    busy={tab.saving}
    onclose={() => (tab.preview = null)}
  >
    {#each tab.preview as statement, index (index)}
      <div class="mb-2 rounded-md border border-zinc-200 p-2 dark:border-zinc-800">
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

    {#snippet footer()}
      <button class="btn ml-auto" onclick={() => (tab.preview = null)}>Cerrar</button>
      <button class="btn btn-primary" disabled={tab.saving} onclick={() => tab.save()}>
        {#if tab.saving}<span class="spinner"></span>{/if}
        Guardar
      </button>
    {/snippet}
  </Modal>
{/if}
