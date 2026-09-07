<script lang="ts">
  /**
   * "Qué puede hacer este rol": mismo dato que la matriz, pivotado para un solo rol y las tres
   * familias de objetos juntas, agrupadas como secciones. De solo lectura, igual que la matriz.
   */
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import type { EffectivePrivilege } from "./ipc";
  import type { RoleCapabilitiesTab } from "./role-capabilities.svelte";

  let { tab }: { tab: RoleCapabilitiesTab } = $props();

  interface Row {
    object: string;
    granted: EffectivePrivilege[];
  }

  function rowsOf(effective: EffectivePrivilege[]): Row[] {
    const byObject = new Map<string, EffectivePrivilege[]>();
    for (const row of effective) {
      if (!row.granted) continue;
      (byObject.get(row.object) ?? byObject.set(row.object, []).get(row.object)!).push(row);
    }
    // Los objetos sin ningún permiso también se listan, para que "sin permisos" sea explícito y no
    // un hueco en la lista.
    const seen = new Set(effective.map((row) => row.object));
    return [...seen].map((object) => ({ object, granted: byObject.get(object) ?? [] }));
  }

  const tables = $derived(rowsOf(tab.tables));
  const sequences = $derived(rowsOf(tab.sequences));
  const functions = $derived(rowsOf(tab.functions));

  async function changeSchema(schema: string) {
    tab.schema = schema;
    await tab.load();
  }
</script>

{#snippet section(title: string, icon: "table" | "sequence" | "function", rows: Row[])}
  <div class="card-head sticky top-0 z-10">
    <Icon name={icon} size={12} />
    <span class="card-title">{title}</span>
    <span class="seg-count">{rows.length}</span>
  </div>
  <table class="list-table">
    <tbody>
      {#each rows as row (row.object)}
        <tr>
          <td class="w-56 font-medium whitespace-nowrap text-zinc-800 dark:text-zinc-100">{row.object}</td>
          <td>
            {#if row.granted.length === 0}
              <span class="muted italic">— sin permisos —</span>
            {:else}
              <div class="flex flex-wrap gap-1.5">
                {#each row.granted as grant (grant.privilege)}
                  <span
                    class="tag {grant.direct ? 'tag-ok' : 'tag-warn'}"
                    title={grant.direct ? "Otorgado directo" : "Heredado por membresía"}
                  >
                    {grant.privilege}
                    {#if !grant.direct}<span class="opacity-70">· heredado</span>{/if}
                  </span>
                {/each}
              </div>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{/snippet}

<div class="flex h-full flex-col">
  <div class="toolbar divider-b gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Esquema</span>
      <select
        class="field"
        value={tab.schema}
        onchange={(event) => changeSchema(event.currentTarget.value)}
      >
        {#each tab.schemas as schema (schema)}
          <option value={schema}>{schema}</option>
        {/each}
      </select>
    </label>

    <button class="btn btn-icon ml-auto" title="Volver a leer" onclick={() => tab.load()}>
      <Icon name="refresh" size={12} />
    </button>
  </div>

  {#if tab.error}
    <Alert tone="bad" onclose={() => (tab.error = null)}>{tab.error}</Alert>
  {/if}

  {#if !tab.loading && tables.length === 0 && sequences.length === 0 && functions.length === 0}
    <Empty icon="role" title="Sin objetos" hint="Este esquema no tiene nada que {tab.role} pueda tocar." />
  {:else}
    <div class="min-h-0 flex-1 overflow-auto">
      {@render section("Tablas", "table", tables)}
      {@render section("Secuencias", "sequence", sequences)}
      {@render section("Funciones", "function", functions)}
    </div>
  {/if}
</div>
