<script lang="ts">
  import Alert from "./Alert.svelte";
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { count, decimal } from "./format";
  import {
    describeError,
    historyClear,
    historyRecent,
    historySearch,
    type HistoryEntry,
  } from "./ipc";

  let {
    profileId,
    onpick,
  }: {
    profileId: string;
    /** Trae la consulta elegida al editor. */
    onpick: (sql: string) => void;
  } = $props();

  let entries = $state<HistoryEntry[]>([]);
  let search = $state("");
  let error = $state<string | null>(null);
  let onlyThisServer = $state(true);
  let confirmClear = $state(false);

  /**
   * La búsqueda va al servidor y no filtra en memoria: el historial crece sin techo y traerlo
   * entero para filtrar acá sería traer todo lo que uno ejecutó desde que instaló la aplicación.
   */
  async function load() {
    error = null;
    try {
      entries =
        search.trim() === ""
          ? await historyRecent(onlyThisServer ? profileId : undefined, 200)
          : await historySearch(search.trim(), 200);
    } catch (failure) {
      error = describeError(failure);
    }
  }

  $effect(() => {
    // Se relee al cambiar el filtro; el texto se busca al soltar Enter, no en cada tecla.
    onlyThisServer;
    profileId;
    load();
  });

  /**
   * La hora va en 24 horas y no en la del sistema: con `a. m.` detrás, «30/8 01:59 a. m.» no entra
   * en la columna y se parte en dos líneas, que en un panel de 300 píxeles se come justamente el
   * lugar donde tiene que leerse la consulta.
   */
  function when(seconds: number): string {
    const date = new Date(seconds * 1000);
    const time = date.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
    if (new Date().toDateString() === date.toDateString()) return time;
    const day = date.toLocaleDateString(undefined, { day: "2-digit", month: "2-digit" });
    return `${day} ${time}`;
  }
</script>

<!-- Se mide contra el panel y no contra la ventana: esta lista vive tanto en el panel lateral,
     que arranca en 300 píxeles, como en el resultado de una consulta a pantalla completa. -->
<div class="@container/history flex h-full flex-col">
  <div class="divider-b flex flex-wrap items-center gap-2 px-2 py-1.5">
    <!-- Angosto, la caja de búsqueda se queda con su propia línea: compartiéndola con la casilla y
         el botón quedaba de cuarenta píxeles, que no alcanza ni para ver lo que uno escribió. -->
    <div class="relative min-w-0 basis-full @sm/history:flex-1 @sm/history:basis-auto">
      <Icon
        name="search"
        size={13}
        class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-zinc-400"
      />
      <input
        class="field w-full py-1 pl-7"
        placeholder="Buscar en el historial y confirmar con Enter"
        bind:value={search}
        onkeydown={(event) => {
          if (event.key === "Enter") load();
          if (event.key === "Escape") {
            search = "";
            load();
          }
        }}
      />
    </div>

    <label class="check" title="La búsqueda por texto siempre mira el historial completo">
      <input type="checkbox" bind:checked={onlyThisServer} disabled={search.trim() !== ""} />
      Solo este servidor
    </label>

    <button class="btn btn-sm btn-danger-ghost" onclick={() => (confirmClear = true)}>
      <Icon name="trash" size={11} />
      Vaciar
    </button>
  </div>

  {#if error}
    <Alert tone="bad" onclose={() => (error = null)}>{error}</Alert>
  {:else if entries.length === 0}
    <Empty
      icon="clock"
      title={search.trim() === "" ? "Todavía no ejecutaste ninguna consulta" : "Sin coincidencias"}
      hint={search.trim() === ""
        ? "Cada consulta que ejecutes queda acá, con su duración y cuántas filas devolvió."
        : "Probá con otra parte del texto de la consulta."}
    />
  {:else}
    <ul class="min-h-0 flex-1 overflow-auto">
      {#each entries as entry (entry.id)}
        <li>
          <button
            class="group flex w-full items-baseline gap-2 px-3 py-1.5 text-left
                   hover:bg-zinc-100 dark:hover:bg-zinc-700/70"
            title="Traer esta consulta al editor"
            onclick={() => onpick(entry.sql)}
          >
            <span class="shrink-0 text-xs whitespace-nowrap tabular-nums muted">
              {when(entry.startedAt)}
            </span>
            <!-- Lo que salió de un diálogo se marca: el historial ya no es solo lo que uno escribió,
                 es todo lo que la aplicación ejecutó contra el servidor. -->
            {#if entry.source === "dialog"}
              <span class="tag tag-neutral shrink-0" title="Lo aplicó un diálogo de la aplicación">
                diálogo
              </span>
            {/if}
            <span
              class="min-w-0 flex-1 truncate font-mono text-xs
                     {entry.succeeded ? '' : 'text-rose-600 dark:text-rose-400'}"
            >
              {entry.sql.replace(/\s+/g, " ")}
            </span>
            <span class="shrink-0 text-xs tabular-nums muted">
              {#if entry.succeeded}
                <!-- Cuántas filas devolvió es lo primero que sobra cuando el panel es angosto: lo
                     que se busca en la lista es la consulta, y el tiempo dice más que el conteo. -->
                {#if entry.rowCount !== null}
                  <span class="hidden @sm/history:inline">{count(entry.rowCount)} filas ·</span>
                {/if}
                {decimal(entry.seconds * 1000, 0)} ms
              {:else}
                <span class="tag tag-bad">falló</span>
              {/if}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if confirmClear}
  <Confirm
    title="Vaciar el historial"
    message="Se borran todas las consultas registradas, de todos los servidores. No se toca nada en la base de datos."
    confirmLabel="Vaciar"
    onconfirm={async () => {
      confirmClear = false;
      try {
        await historyClear();
      } catch (failure) {
        error = describeError(failure);
      }
      await load();
    }}
    onclose={() => (confirmClear = false)}
  />
{/if}
