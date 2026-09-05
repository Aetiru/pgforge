<script lang="ts">
  /**
   * La lista de marcadores, agrupada por servidor: forma de `SavedPanel.svelte` —filtro en memoria,
   * `Alert`/`Empty`/`<ul>`, `row-actions` que aparecen al pasar por encima, `Confirm` antes de
   * borrar—. `bookmarks.entries` ya llega ordenado por servidor desde el backend
   * (`BookmarkStore::list`), así que agrupar acá es solo partir la lista donde cambia el `profileId`.
   */
  import Alert from "./Alert.svelte";
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { lookOf } from "./badges";
  import { bookmarks } from "./bookmarks.svelte";
  import { explorer } from "./explorer.svelte";
  import type { Bookmark } from "./ipc";

  let { onopen }: { onopen: (bookmark: Bookmark) => void } = $props();

  let filter = $state("");
  let confirmDelete = $state<Bookmark | null>(null);

  const shown = $derived.by(() => {
    const text = filter.trim().toLowerCase();
    if (!text) return bookmarks.entries;
    return bookmarks.entries.filter(
      (entry) =>
        entry.name.toLowerCase().includes(text) ||
        entry.schema.toLowerCase().includes(text) ||
        (entry.label ?? "").toLowerCase().includes(text),
    );
  });

  interface Group {
    profileId: string;
    server: string;
    items: Bookmark[];
  }

  /** Parte la lista donde cambia el servidor, y le pone el nombre que tenga en `explorer.profiles`. */
  const groups = $derived.by<Group[]>(() => {
    const out: Group[] = [];
    for (const entry of shown) {
      const last = out.at(-1);
      if (last && last.profileId === entry.profileId) {
        last.items.push(entry);
      } else {
        const server =
          explorer.profiles.find((profile) => profile.id === entry.profileId)?.name ??
          entry.profileId;
        out.push({ profileId: entry.profileId, server, items: [entry] });
      }
    }
    return out;
  });

  async function remove(entry: Bookmark) {
    await bookmarks.remove(entry.id);
    confirmDelete = null;
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
        placeholder="Filtrar marcadores"
        bind:value={filter}
        onkeydown={(event) => {
          if (event.key === "Escape") filter = "";
        }}
      />
    </div>
  </div>

  {#if bookmarks.error}
    <Alert tone="bad" onclose={() => (bookmarks.error = null)}>{bookmarks.error}</Alert>
  {/if}

  {#if shown.length === 0}
    <Empty
      icon="star"
      title={bookmarks.entries.length === 0 ? "Todavía no marcaste nada" : "Sin coincidencias"}
      hint={bookmarks.entries.length === 0
        ? "Marcá una tabla, vista, función o procedimiento desde el árbol o desde su detalle para tenerla siempre a mano acá."
        : "Probá con otra parte del nombre, del esquema o del alias."}
    />
  {:else}
    <ul class="min-h-0 flex-1 overflow-auto">
      {#each groups as group (group.profileId)}
        <li class="divider-b px-3 py-1 text-[11px] font-semibold tracking-wide uppercase muted">
          {group.server}
        </li>
        {#each group.items as entry (entry.id)}
          {@const look = lookOf(entry.kind)}
          <li
            class="group flex items-center gap-2 px-3 py-1.5 hover:bg-zinc-100
                   dark:hover:bg-zinc-700/70"
          >
            <button
              class="flex min-w-0 flex-1 items-center gap-1.5 text-left"
              title="Revelar en el árbol"
              onclick={() => onopen(entry)}
            >
              <Icon name={look.icon} size={13} class={look.tone} />
              <span class="min-w-0 flex-1 truncate text-sm">{entry.label ?? entry.name}</span>
              <span class="min-w-0 shrink-[100] truncate text-xs muted">{entry.schema}</span>
            </button>

            <div class="row-actions shrink-0">
              <button
                class="btn btn-icon btn-sm btn-danger-ghost"
                title="Quitar el marcador"
                aria-label="Quitar el marcador"
                onclick={() => (confirmDelete = entry)}
              >
                <Icon name="trash" size={11} />
              </button>
            </div>
          </li>
        {/each}
      {/each}
    </ul>
  {/if}
</div>

{#if confirmDelete}
  <Confirm
    title="Quitar de marcadores"
    message="¿Quitar «{confirmDelete.label ?? confirmDelete.name}» de la sección fija? El objeto no se toca en el servidor."
    confirmLabel="Quitar"
    onconfirm={() => confirmDelete && remove(confirmDelete)}
    onclose={() => (confirmDelete = null)}
  />
{/if}
