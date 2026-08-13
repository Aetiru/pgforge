<script lang="ts">
  /** Las columnas de la tabla elegida, con lo que se puede hacer con cada una. */
  import Icon from "../Icon.svelte";
  import type { TableColumn, TableShape } from "../ipc";
  import Card from "./Card.svelte";

  let {
    shape,
    loading = false,
    error = null,
    blocked = {},
    onnew,
    onedit,
    ondrop,
  }: {
    shape: TableShape | null;
    loading?: boolean;
    error?: string | null;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    onedit: (column: TableColumn) => void;
    ondrop: (column: string) => void;
  } = $props();
</script>

<Card title="Columnas" loading={loading ? "Leyendo columnas…" : null} {error}>
  {#snippet actions()}
    {#if shape}
      <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
        <Icon name="plus" size={11} />
        Columna
      </button>
    {/if}
  {/snippet}

  {#if shape}
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-px whitespace-nowrap">Nombre</th>
          <th class="w-px whitespace-nowrap">Tipo</th>
          <th class="w-full">Valor por omisión</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each shape.columns as column (column.name)}
          <tr class="group">
            <td class="w-px font-medium whitespace-nowrap">
              {column.name}
              {#if column.notNull}
                <span class="tag tag-neutral ml-1">NOT NULL</span>
              {/if}
            </td>
            <td class="w-px font-mono text-xs whitespace-nowrap muted">{column.typeName}</td>
            <td class="max-w-0 truncate text-xs muted">
              {column.default ?? (column.generated ? "generada por el servidor" : "—")}
            </td>
            <td class="w-28">
              <div class="row-actions">
                {#if !column.generated}
                  <button
                    class="btn btn-ghost btn-icon size-6"
                    title="Editar la columna"
                    aria-label="Editar la columna"
                    {...blocked}
                    onclick={() => onedit(column)}
                  >
                    <Icon name="edit" size={12} />
                  </button>
                {/if}
                <button
                  class="btn btn-danger-ghost btn-icon size-6"
                  title="Eliminar la columna"
                  aria-label="Eliminar la columna"
                  {...blocked}
                  onclick={() => ondrop(column.name)}
                >
                  <Icon name="trash" size={12} />
                </button>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</Card>
