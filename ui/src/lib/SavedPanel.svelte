<script lang="ts">
  import Alert from "./Alert.svelte";
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { describeError, savedDelete, savedList, type SavedQuery } from "./ipc";
  import { forgetSaved } from "./query.svelte";

  let {
    onpick,
    reload = 0,
  }: {
    /** Trae la consulta elegida al editor. */
    onpick: (saved: SavedQuery) => void;
    /** Cambia cuando alguien guardó desde afuera del panel, para volver a leer la lista. */
    reload?: number;
  } = $props();

  let entries = $state<SavedQuery[]>([]);
  let filter = $state("");
  let error = $state<string | null>(null);
  let confirmDelete = $state<SavedQuery | null>(null);

  /**
   * El filtro es en memoria, al revés del historial: las consultas guardadas son decenas —las puso
   * el usuario a mano, una por una— y traerlas todas cuesta menos que ir al backend por cada tecla.
   */
  const shown = $derived.by(() => {
    const text = filter.trim().toLowerCase();
    if (!text) return entries;
    return entries.filter(
      (entry) =>
        entry.name.toLowerCase().includes(text) || entry.sql.toLowerCase().includes(text),
    );
  });

  async function load() {
    error = null;
    try {
      entries = await savedList();
    } catch (failure) {
      error = describeError(failure);
    }
  }

  $effect(() => {
    reload;
    load();
  });

  async function remove(saved: SavedQuery) {
    try {
      await savedDelete(saved.id);
      // Las pestañas que salieron de esta dejan de apuntarle: si no, su «Guardar» quedaría
      // intentando reescribir algo que ya no está.
      forgetSaved(saved.id);
      await load();
    } catch (failure) {
      error = describeError(failure);
    } finally {
      confirmDelete = null;
    }
  }

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleDateString(undefined, {
      day: "2-digit",
      month: "2-digit",
      year: "2-digit",
    });
  }
</script>

<div class="flex h-full flex-col">
  <div class="divider-b flex items-center gap-2 px-2 py-1.5">
    <div class="relative flex-1">
      <Icon
        name="search"
        size={13}
        class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-zinc-400"
      />
      <input
        class="field w-full py-1 pl-7"
        placeholder="Filtrar por nombre o por lo que dice la consulta"
        bind:value={filter}
        onkeydown={(event) => {
          if (event.key === "Escape") filter = "";
        }}
      />
    </div>

    <button class="btn btn-sm btn-ghost" title="Volver a leer la lista" onclick={load}>
      <Icon name="refresh" size={11} />
    </button>
  </div>

  {#if error}
    <Alert tone="bad" onclose={() => (error = null)}>{error}</Alert>
  {:else if shown.length === 0}
    <Empty
      icon="save"
      title={entries.length === 0 ? "Todavía no guardaste ninguna consulta" : "Sin coincidencias"}
      hint={entries.length === 0
        ? "«Guardar» le pone nombre a lo que está en el editor y la deja acá, en esta máquina, para abrirla cuando haga falta."
        : "Probá con otra parte del nombre o del texto."}
    />
  {:else}
    <ul class="min-h-0 flex-1 overflow-auto">
      {#each shown as entry (entry.id)}
        <li
          class="group flex items-center gap-2 px-3 py-1.5 hover:bg-zinc-100
                 dark:hover:bg-zinc-700/70"
        >
          <button
            class="flex min-w-0 flex-1 items-baseline gap-2 text-left"
            title="Traer esta consulta al editor"
            onclick={() => onpick(entry)}
          >
            <span class="shrink-0 truncate text-sm">{entry.name}</span>
            <span class="min-w-0 flex-1 truncate font-mono text-xs muted">
              {entry.sql.replace(/\s+/g, " ")}
            </span>
          </button>

          <span class="shrink-0 text-xs tabular-nums muted" title="Última vez que se guardó">
            {when(entry.updatedAt)}
          </span>

          <div class="row-actions shrink-0">
            <button
              class="btn btn-icon btn-sm btn-danger-ghost"
              title="Borrar «{entry.name}»"
              aria-label="Borrar"
              onclick={() => (confirmDelete = entry)}
            >
              <Icon name="trash" size={11} />
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if confirmDelete}
  <Confirm
    title="Borrar «{confirmDelete.name}»"
    message="La consulta guardada se borra de esta máquina. Lo que esté abierto en el editor no se toca."
    confirmLabel="Borrar"
    onconfirm={() => confirmDelete && remove(confirmDelete)}
    onclose={() => (confirmDelete = null)}
  />
{/if}
