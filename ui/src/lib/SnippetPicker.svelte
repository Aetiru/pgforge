<script lang="ts">
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Sql from "./Sql.svelte";
  import type { Snippet, SnippetUse } from "./ipc";
  import { filterSnippets, sortSnippets } from "./snippet-list";
  import { autoVariablesOf, previewBody, variablesOf } from "./snippet-vars";

  /**
   * Elegir una abreviatura buscándola, con el texto completo a la vista antes de insertarla: con
   * veinte abreviaturas nadie se acuerda de cómo se llamaba la de paginar, pero sí de qué quería
   * hacer. Es la misma mecánica que el buscador de objetos —se cierra con clic afuera, no hay nada
   * escrito que perder—, y elegir **inserta** por el mismo camino que el tabulador, así que las
   * variables se preguntan igual y el uso se cuenta igual.
   *
   * Recibe solo las abreviaturas que valen en este servidor y esta base (ver `visibleFor`): ofrecer
   * una que después no se puede usar ahí es peor que no ofrecerla.
   */
  let {
    items,
    usage,
    onpick,
    onclose,
  }: {
    items: Snippet[];
    usage: Record<string, SnippetUse>;
    onpick: (item: Snippet) => void;
    onclose: () => void;
  } = $props();

  let query = $state("");
  let cursor = $state(0);
  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    input?.focus();
  });

  // Las más usadas arriba: es lo que casi siempre se busca, y sin usos queda el orden por nombre.
  const shown = $derived(sortSnippets(filterSnippets(items, query), "uses", usage));
  const current = $derived(shown[cursor] ?? null);

  // Al cambiar la búsqueda la selección vuelve al primero, o quedaría apuntando a una fila que ya no está.
  $effect(() => {
    void query;
    cursor = 0;
  });

  const asks = $derived(current ? variablesOf(current.body) : []);
  const autos = $derived(current ? autoVariablesOf(current.body) : []);

  function onkeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        onclose();
        break;
      case "ArrowDown":
        event.preventDefault();
        cursor = shown.length === 0 ? 0 : (cursor + 1) % shown.length;
        break;
      case "ArrowUp":
        event.preventDefault();
        cursor = shown.length === 0 ? 0 : (cursor - 1 + shown.length) % shown.length;
        break;
      case "Enter":
        event.preventDefault();
        if (current) onpick(current);
        break;
    }
  }

  const KIND: Record<string, string> = {
    text: "texto",
    list: "lista",
    date: "fecha",
    number: "número",
    table: "tabla del catálogo",
    column: "columna del catálogo",
  };
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40 flex justify-center bg-zinc-950/40 p-4 pt-[10vh] backdrop-blur-[2px]"
  onclick={(event) => event.target === event.currentTarget && onclose()}
  {onkeydown}
>
  <div class="card flex max-h-[75vh] w-full max-w-4xl flex-col overflow-hidden shadow-2xl">
    <div class="divider-b flex items-center gap-2 px-3 py-2">
      <Icon name="search" size={14} class="shrink-0 text-zinc-400" />
      <input
        bind:this={input}
        bind:value={query}
        class="w-full bg-transparent text-sm outline-none placeholder:text-zinc-400"
        placeholder="Buscar una abreviatura por nombre, descripción o texto…"
        aria-label="Buscar una abreviatura para insertarla"
        role="combobox"
        aria-expanded="true"
        aria-controls="snippet-picker-list"
        aria-activedescendant={current ? `snippet-picker-${cursor}` : undefined}
      />
      <kbd class="shrink-0 text-[10px] muted">Esc</kbd>
    </div>

    <div class="grid min-h-0 flex-1 grid-cols-1 overflow-hidden sm:grid-cols-[minmax(0,260px)_minmax(0,1fr)]">
      <div id="snippet-picker-list" role="listbox" aria-label="Abreviaturas" class="min-h-0 overflow-auto py-1 sm:border-r sm:border-zinc-200 sm:dark:border-zinc-700">
        {#if shown.length === 0}
          <Empty
            icon="search"
            title={items.length === 0 ? "No hay abreviaturas para este servidor" : "Sin coincidencias"}
            hint={items.length === 0
              ? "Crea una desde «Abreviaturas…» o guardando una selección."
              : `Ninguna abreviatura menciona «${query}».`}
          />
        {:else}
          {#each shown as item, index (item.id)}
            <button
              id="snippet-picker-{index}"
              role="option"
              aria-selected={index === cursor}
              class="flex w-full items-baseline gap-2 px-3 py-1.5 text-left text-sm
                     {index === cursor ? 'bg-blue-500/10' : ''}"
              onclick={() => onpick(item)}
              onmouseenter={() => (cursor = index)}
            >
              <span class="shrink-0 font-mono">{item.abbreviation}</span>
              <span class="min-w-0 flex-1 truncate text-xs muted">{item.description}</span>
              {#if (usage[item.id ?? ""]?.uses ?? 0) > 0}
                <span class="shrink-0 text-[10px] muted" title="Veces que se expandió">
                  {usage[item.id ?? ""]?.uses}
                </span>
              {/if}
            </button>
          {/each}
        {/if}
      </div>

      <div class="hidden min-h-0 flex-col gap-2 overflow-auto p-3 sm:flex">
        {#if current}
          <div class="flex items-baseline gap-2">
            <span class="font-mono text-sm font-semibold">{current.abbreviation}</span>
            {#if current.category}<span class="tag tag-neutral">{current.category}</span>{/if}
            <span class="min-w-0 flex-1 truncate text-xs muted">{current.description}</span>
          </div>
          <div class="select-text rounded-md border border-zinc-200 bg-zinc-50 p-2 dark:border-zinc-700 dark:bg-zinc-900">
            <Sql code={previewBody(current.body)} />
          </div>
          {#if asks.length > 0}
            <div>
              <div class="label mb-1">Va a preguntar</div>
              <ul class="flex flex-col gap-0.5 text-xs">
                {#each asks as variable (variable.name)}
                  <li>
                    <span class="font-mono">{variable.name}</span>
                    <span class="muted">· {KIND[variable.kind]}{variable.kind === "list" ? `: ${(variable.options ?? []).join(", ")}` : variable.default ? ` (por omisión ${variable.default})` : ""}</span>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
          {#if autos.length > 0}
            <p class="text-xs muted">
              Se llena sola: {autos.map((name) => name).join(", ")}.
            </p>
          {/if}
        {/if}
      </div>
    </div>

    <div class="divider-t flex items-center gap-3 px-3 py-1.5 text-[11px] muted">
      <span><kbd>↑</kbd> <kbd>↓</kbd> navegar</span>
      <span><kbd>Enter</kbd> insertar</span>
      <span class="ml-auto">{shown.length} de {items.length}</span>
    </div>
  </div>
</div>
