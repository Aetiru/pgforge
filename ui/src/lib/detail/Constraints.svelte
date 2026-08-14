<script lang="ts">
  /** Las restricciones de la tabla, con la definición tal como la devuelve `pg_get_constraintdef`. */
  import Icon from "../Icon.svelte";
  import type { ConstraintInfo } from "../ipc";
  import Card from "./Card.svelte";

  let {
    constraints,
    loading = false,
    error = null,
    canCreate = false,
    blocked = {},
    onnew,
    ondrop,
  }: {
    constraints: ConstraintInfo[] | null;
    loading?: boolean;
    error?: string | null;
    canCreate?: boolean;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    ondrop: (name: string) => void;
  } = $props();
</script>

<Card
  title="Restricciones"
  loading={loading ? "Leyendo restricciones…" : null}
  {error}
  empty={constraints && constraints.length === 0 ? "No tiene restricciones propias." : null}
>
  {#snippet actions()}
    {#if canCreate}
      <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
        <Icon name="plus" size={11} />
        Restricción
      </button>
    {/if}
  {/snippet}

  {#if constraints}
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-px whitespace-nowrap">Nombre</th>
          <th class="w-full">Definición</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each constraints as constraint (constraint.oid)}
          <tr class="group">
            <td class="w-px font-medium whitespace-nowrap">
              {constraint.name}
              <span class="tag tag-neutral ml-1">{constraint.kind}</span>
            </td>
            <td class="max-w-0 truncate font-mono text-xs muted" title={constraint.definition}>
              {constraint.definition}
            </td>
            <td class="w-16">
              <div class="row-actions">
                <button
                  class="btn btn-danger-ghost btn-icon size-6"
                  title="Eliminar la restricción"
                  aria-label="Eliminar la restricción"
                  {...blocked}
                  onclick={() => ondrop(constraint.name)}
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
