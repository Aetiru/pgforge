<script lang="ts">
  /** Los índices propios de la tabla; los que vienen de una restricción se ven igual, con su tono. */
  import Icon from "../Icon.svelte";
  import type { IndexInfo } from "../ipc";
  import Card from "./Card.svelte";

  let {
    indexes,
    loading = false,
    error = null,
    canCreate = false,
    blocked = {},
    onnew,
    ondrop,
  }: {
    indexes: IndexInfo[] | null;
    loading?: boolean;
    error?: string | null;
    canCreate?: boolean;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    ondrop: (name: string) => void;
  } = $props();
</script>

<Card
  title="Índices"
  loading={loading ? "Leyendo índices…" : null}
  {error}
  empty={indexes && indexes.length === 0 ? "No tiene índices propios." : null}
>
  {#snippet actions()}
    {#if canCreate}
      <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
        <Icon name="plus" size={11} />
        Índice
      </button>
    {/if}
  {/snippet}

  {#if indexes}
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-px whitespace-nowrap">Nombre</th>
          <th class="w-full">Definición</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each indexes as index (index.oid)}
          <tr class="group">
            <td class="w-px font-medium whitespace-nowrap">
              {index.name}
              {#if index.primary}
                <span class="tag tag-info ml-1">primario</span>
              {:else if index.unique}
                <span class="tag tag-info ml-1">único</span>
              {/if}
              {#if !index.valid}
                <span class="tag tag-bad ml-1">inválido</span>
              {/if}
            </td>
            <td class="max-w-0 truncate font-mono text-xs muted" title={index.definition}>
              {index.definition}
            </td>
            <td class="w-16">
              <div class="row-actions">
                <button
                  class="btn btn-danger-ghost btn-icon size-6"
                  title="Eliminar el índice"
                  aria-label="Eliminar el índice"
                  {...blocked}
                  onclick={() => ondrop(index.name)}
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
