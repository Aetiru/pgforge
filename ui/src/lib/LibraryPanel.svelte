<script lang="ts">
  import Empty from "./Empty.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
  import SavedPanel from "./SavedPanel.svelte";
  import { describeError, type SavedQuery } from "./ipc";
  import { QueryTab, openQuery } from "./query.svelte";
  import { tabs } from "./tabs.svelte";

  /**
   * Historial y consultas guardadas, fuera de una pestaña de consulta.
   *
   * Las dos listas ya existían, pero solo adentro de `QueryPanel`: para volver a correr lo de ayer
   * había que abrir una consulta primero —y elegir contra qué base, que es justo el dato que se
   * quería sacar de la lista—. Acá viven al lado del árbol, y elegir una fila la trae a la pestaña
   * que se está mirando o abre una nueva contra el mismo servidor y base con que se guardó.
   */
  let {
    profileId,
    database,
    onerror,
  }: {
    /** Servidor con el que se filtra el historial; `null` si no hay ninguno a mano. */
    profileId: string | null;
    database: string | null;
    onerror: (message: string) => void;
  } = $props();

  let mode = $state<"history" | "saved">("history");

  /**
   * Dónde cae lo elegido. Manda la pestaña de consulta que se está mirando —traer el SQL a donde
   * uno ya está trabajando es lo que se espera— y, si no hay ninguna, se abre una nueva contra el
   * servidor y la base con que se ejecutó o se guardó.
   */
  async function apply(sql: string, saved: SavedQuery | null, target: { profileId: string | null; database: string | null }) {
    const current = tabs.current;
    if (current instanceof QueryTab) {
      if (saved) current.applySaved(saved);
      else current.sql = sql;
      return;
    }

    const server = target.profileId ?? profileId;
    if (!server) {
      onerror("Conectá un servidor para abrir esta consulta.");
      return;
    }
    try {
      const tab = await openQuery(server, target.database ?? database ?? "", saved?.name ?? "Consulta");
      if (saved) tab.applySaved(saved);
      else tab.sql = sql;
    } catch (error) {
      onerror(describeError(error));
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex items-center gap-1 px-2 py-2">
    <div class="seg" role="tablist">
      <button
        class="seg-item"
        role="tab"
        aria-selected={mode === "history"}
        onclick={() => (mode = "history")}>Historial</button
      >
      <button
        class="seg-item"
        role="tab"
        aria-selected={mode === "saved"}
        onclick={() => (mode = "saved")}>Guardadas</button
      >
    </div>
  </div>

  <div class="min-h-0 flex-1">
    {#if mode === "history"}
      {#if profileId}
        {#key profileId}
          <HistoryPanel
            {profileId}
            onpick={(sql) => void apply(sql, null, { profileId, database })}
          />
        {/key}
      {:else}
        <Empty
          icon="clock"
          title="Sin servidor"
          hint="El historial se filtra por servidor. Conectá uno o elegí algo en el árbol."
        />
      {/if}
    {:else}
      <SavedPanel
        onpick={(saved) =>
          void apply(saved.sql, saved, { profileId: saved.profileId, database: saved.database })}
      />
    {/if}
  </div>
</div>
