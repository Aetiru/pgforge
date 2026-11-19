<script lang="ts">
  import { tick } from "svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { explorer } from "./explorer.svelte";
  import { QueryTab } from "./query.svelte";
  import { tabs } from "./tabs.svelte";

  /**
   * Cambiar de pestaña escribiendo, en vez de recorrer la tira con el mouse. Con diez consultas
   * abiertas la tira se desborda y los títulos se parecen; acá se filtra por título, servidor o base
   * y el orden es el de uso —lo último que se tocó arriba—. Solo ofrece lo que ya está abierto, así
   * que no pregunta nada al servidor.
   */
  let { onpick, onclose }: { onpick: (key: string) => void; onclose: () => void } = $props();

  let query = $state("");
  let cursor = $state(0);
  let input = $state<HTMLInputElement | null>(null);

  const serverOf = (profileId: string) =>
    explorer.profiles.find((profile) => profile.id === profileId)?.name ?? "";

  const items = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    return tabs.byRecency
      .map((tab) => ({ tab, server: serverOf(tab.profileId) }))
      .filter(
        ({ tab, server }) =>
          needle === "" ||
          `${tab.title} ${server} ${tab.database}`.toLowerCase().includes(needle),
      );
  });

  $effect(() => {
    query;
    cursor = 0;
  });

  $effect(() => {
    void tick().then(() => input?.focus());
  });

  function onkeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        onclose();
        break;
      case "ArrowDown":
        event.preventDefault();
        cursor = items.length === 0 ? 0 : (cursor + 1) % items.length;
        break;
      case "ArrowUp":
        event.preventDefault();
        cursor = items.length === 0 ? 0 : (cursor - 1 + items.length) % items.length;
        break;
      case "Enter": {
        event.preventDefault();
        const chosen = items[cursor];
        if (chosen) onpick(chosen.tab.key);
        break;
      }
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40 flex justify-center bg-zinc-950/40 p-4 pt-[12vh] backdrop-blur-[2px]"
  onclick={(event) => event.target === event.currentTarget && onclose()}
  {onkeydown}
>
  <div class="card flex max-h-[70vh] w-full max-w-xl flex-col overflow-hidden shadow-2xl">
    <div class="divider-b flex items-center gap-2 px-3 py-2">
      <Icon name="search" size={14} class="shrink-0 text-zinc-400" />
      <input
        bind:this={input}
        bind:value={query}
        class="w-full bg-transparent text-sm outline-none placeholder:text-zinc-400"
        placeholder="Buscar entre las pestañas abiertas…"
        aria-label="Buscar pestaña"
      />
      <kbd class="shrink-0 text-[10px] muted">Esc</kbd>
    </div>

    <div class="min-h-0 flex-1 overflow-auto py-1">
      {#if items.length === 0}
        <Empty
          icon="search"
          title="Ninguna pestaña coincide"
          hint="Se busca por título, servidor y base entre las pestañas abiertas."
        />
      {:else}
        {#each items as { tab, server }, index (tab.key)}
          <button
            class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm
                   {index === cursor ? 'bg-blue-500/10' : ''}"
            onclick={() => onpick(tab.key)}
            onmouseenter={() => (cursor = index)}
          >
            <Icon name="table" size={12} class="shrink-0 text-zinc-400" />
            <span class="min-w-0 flex-1 truncate">{tab.title}</span>
            {#if tab instanceof QueryTab && tab.dirty}
              <span class="size-2 shrink-0 rounded-full bg-amber-500" title="Cambios sin guardar"></span>
            {/if}
            <span class="min-w-0 shrink-[100] truncate text-xs muted">
              {server}{tab.database ? ` / ${tab.database}` : ""}
            </span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>
