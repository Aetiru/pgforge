<script lang="ts">
  /**
   * Los privilegios del objeto: los suyos, los acotados a columnas y —solo en un esquema— los que
   * van a recibir los objetos que se creen adentro.
   */
  import Icon from "../Icon.svelte";
  import { pairKey, type ColumnGroup, type PrivilegeGroup } from "../detail-node";
  import type { DefaultGrant } from "../ipc";
  import Card from "./Card.svelte";

  let {
    groups,
    columnGroups = [],
    defaultGrants = [],
    loading = false,
    error = null,
    blocked = {},
    onnew,
    onedit,
    onrevoke,
  }: {
    groups: PrivilegeGroup[];
    columnGroups?: ColumnGroup[];
    /** Solo se muestran en un esquema: los privilegios por omisión se guardan por esquema. */
    defaultGrants?: DefaultGrant[];
    loading?: boolean;
    error?: string | null;
    blocked?: { disabled?: boolean; title?: string };
    onnew: () => void;
    onedit: (group: PrivilegeGroup) => void;
    onrevoke: (group: PrivilegeGroup) => void;
  } = $props();
</script>

<Card
  title="Privilegios"
  loading={loading ? "Leyendo privilegios…" : null}
  {error}
  empty={groups.length === 0
    ? "Nadie tiene privilegios propios: rige el default (el dueño puede todo)."
    : null}
>
  {#snippet actions()}
    <button class="btn btn-sm ml-auto" {...blocked} onclick={onnew}>
      <Icon name="plus" size={11} />
      Privilegio
    </button>
  {/snippet}

  <table class="list-table">
    <thead>
      <tr>
        <th class="w-px whitespace-nowrap">Rol</th>
        <th class="w-full">Privilegios</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each groups as group (group.grantee)}
        <tr class="group">
          <td class="w-px font-medium whitespace-nowrap">{group.grantee}</td>
          <td>
            <span class="flex flex-wrap gap-1">
              <!-- Un mismo privilegio puede venir dos veces con otorgantes distintos: la clave es
                   la posición y no el nombre. -->
              {#each group.privileges as privilege, position (position)}
                <span class="tag tag-neutral font-mono">{privilege.toUpperCase()}</span>
              {/each}
              {#if group.grantable}
                <span class="tag tag-info">con GRANT OPTION</span>
              {/if}
            </span>
          </td>
          <td class="w-24">
            <div class="row-actions">
              <button
                class="btn btn-ghost btn-icon size-6"
                title="Editar los privilegios"
                aria-label="Editar los privilegios"
                {...blocked}
                onclick={() => onedit(group)}
              >
                <Icon name="edit" size={12} />
              </button>
              <button
                class="btn btn-danger-ghost btn-icon size-6"
                title="Revocar todo"
                aria-label="Revocar todo"
                {...blocked}
                onclick={() => onrevoke(group)}
              >
                <Icon name="trash" size={12} />
              </button>
            </div>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</Card>

{#if columnGroups.length > 0}
  <div class="card mt-3 overflow-hidden">
    <div class="card-head">
      <span class="card-title">Acotados a columnas</span>
      <span class="seg-count">{columnGroups.length}</span>
    </div>
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-px whitespace-nowrap">Columna</th>
          <th class="w-px whitespace-nowrap">Rol</th>
          <th class="w-full">Privilegios</th>
        </tr>
      </thead>
      <tbody>
        {#each columnGroups as group (pairKey(group.column, group.grantee))}
          <tr>
            <td class="w-px font-mono whitespace-nowrap">{group.column}</td>
            <td class="w-px font-medium whitespace-nowrap">{group.grantee}</td>
            <td>
              <span class="flex flex-wrap gap-1">
                {#each group.privileges as privilege, position (position)}
                  <span class="tag tag-neutral font-mono">{privilege}</span>
                {/each}
              </span>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

{#if defaultGrants.length > 0}
  <div class="card mt-3 overflow-hidden">
    <div class="card-head">
      <span class="card-title">Por omisión</span>
      <span class="text-xs muted">lo que van a recibir los objetos que se creen acá</span>
    </div>
    <table class="list-table">
      <thead>
        <tr>
          <th class="w-px whitespace-nowrap">Cuando crea</th>
          <th class="w-px whitespace-nowrap">Sobre</th>
          <th class="w-px whitespace-nowrap">Rol</th>
          <th class="w-full">Privilegio</th>
        </tr>
      </thead>
      <tbody>
        {#each defaultGrants as grant, position (position)}
          <tr>
            <td class="w-px whitespace-nowrap">{grant.owner}</td>
            <td class="w-px whitespace-nowrap">{grant.objects}</td>
            <td class="w-px font-medium whitespace-nowrap">{grant.grantee}</td>
            <td>
              <span class="tag tag-neutral font-mono">{grant.privilege}</span>
              {#if grant.grantable}
                <span class="tag tag-info">con GRANT OPTION</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
