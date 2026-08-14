<script lang="ts">
  /**
   * Una carpeta de conexiones no tiene catálogo que mostrar: lo único que contiene son conexiones,
   * así que el panel es la lista de esas conexiones con lo que se puede hacer con cada una.
   */
  import Icon from "../Icon.svelte";
  import type { Row } from "../explorer.svelte";

  let {
    servers,
    onconnect,
    ondisconnect,
    onedit,
  }: {
    servers: Row[];
    onconnect: (profileId: string) => void;
    ondisconnect: (profileId: string) => void;
    onedit: (profileId: string) => void;
  } = $props();
</script>

<div class="card overflow-hidden">
  <div class="card-head">
    <span class="card-title">Servidores de la carpeta</span>
    <span class="ml-auto text-xs muted">Arrastrá un servidor del árbol para meterlo o sacarlo</span>
  </div>

  <table class="list-table">
    <tbody>
      {#each servers as server (server.profileId)}
        <tr class="group">
          <td class="w-px whitespace-nowrap">
            <span class="flex items-center gap-1.5">
              <span class="dot {server.connected ? 'dot-on' : 'dot-off'}"></span>
              <span class="font-medium">{server.label}</span>
            </span>
          </td>
          <td class="text-xs muted">{server.detail}</td>
          <td class="w-40">
            <div class="row-actions">
              {#if server.connected}
                <button class="btn btn-sm" onclick={() => ondisconnect(server.profileId)}>
                  Desconectar
                </button>
              {:else}
                <button class="btn btn-sm" onclick={() => onconnect(server.profileId)}>
                  Conectar
                </button>
              {/if}
              <button
                class="btn btn-ghost btn-icon size-6"
                title="Editar el servidor"
                aria-label="Editar el servidor"
                onclick={() => onedit(server.profileId)}
              >
                <Icon name="edit" size={12} />
              </button>
            </div>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
