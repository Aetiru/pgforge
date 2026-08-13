<script lang="ts">
  /** Los triggers de la tabla: cuándo se disparan y qué función ejecutan. */
  import Icon from "../Icon.svelte";
  import { triggerSummary } from "../detail-node";
  import type { TriggerInfo } from "../ipc";
  import Card from "./Card.svelte";

  let {
    triggers,
    loading = false,
    error = null,
    canCreate = false,
    blocked = {},
    onnew,
    onedit,
    ondrop,
  }: {
    triggers: TriggerInfo[] | null;
    loading?: boolean;
    error?: string | null;
    canCreate?: boolean;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    onedit: (trigger: TriggerInfo) => void;
    ondrop: (name: string) => void;
  } = $props();
</script>

<Card
  title="Triggers"
  loading={loading ? "Leyendo triggers…" : null}
  {error}
  empty={triggers && triggers.length === 0 ? "No tiene triggers propios." : null}
>
  {#snippet actions()}
    {#if canCreate}
      <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
        <Icon name="plus" size={11} />
        Trigger
      </button>
    {/if}
  {/snippet}

  {#if triggers}
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-px whitespace-nowrap">Nombre</th>
          <th class="w-px whitespace-nowrap">Cuándo</th>
          <th class="w-full">Ejecuta</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each triggers as trigger (trigger.oid)}
          <tr class="group">
            <td class="w-px font-medium whitespace-nowrap">{trigger.name}</td>
            <td class="w-px text-xs whitespace-nowrap muted">{triggerSummary(trigger)}</td>
            <td class="max-w-0 truncate font-mono text-xs muted">
              {trigger.functionSchema}.{trigger.functionName}()
            </td>
            <td class="w-24">
              <div class="row-actions">
                <button
                  class="btn btn-ghost btn-icon size-6"
                  title="Editar el trigger"
                  aria-label="Editar el trigger"
                  {...blocked}
                  onclick={() => onedit(trigger)}
                >
                  <Icon name="edit" size={12} />
                </button>
                <button
                  class="btn btn-danger-ghost btn-icon size-6"
                  title="Eliminar el trigger"
                  aria-label="Eliminar el trigger"
                  {...blocked}
                  onclick={() => ondrop(trigger.name)}
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
