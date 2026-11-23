<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { lookOf } from "./badges";
  import { describeError, treeSearch, type SearchHit } from "./ipc";
  import { filterHits, parseQuery } from "./tree-query";

  /**
   * Buscar un objeto del servidor para dejar su nombre en el editor, sin salir del teclado ni abrir
   * el árbol. Es la misma búsqueda de `SearchModal` —mismo `tree_search`, mismos prefijos de tipo—,
   * pero elegir un resultado **inserta** en vez de abrir una pestaña.
   *
   * `Enter` inserta el nombre calificado; `Mayús`+`Enter`, un `SELECT` con todas las columnas, que es
   * lo mismo que `Mayús`+soltar un objeto del árbol sobre el editor.
   */
  let {
    profileId,
    database,
    onpick,
    onclose,
  }: {
    profileId: string;
    database: string;
    onpick: (hit: SearchHit, withColumns: boolean) => void;
    onclose: () => void;
  } = $props();

  // Texto de `PREFIX_HELP` (tree-query.ts) traducido: la constante del módulo quedaría fija en un idioma.
  const prefixHelp = $derived(
    t("t: tablas · v: vistas · s: esquemas · f: funciones · i: índices · q: secuencias · c: columnas · r: roles · y: tipos"),
  );

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let cursor = $state(0);
  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    input?.focus();
  });

  // Cada tecla pide de nuevo al servidor; el rebote evita mandar una consulta por letra.
  $effect(() => {
    const text = query;
    cursor = 0;

    const parsed = parseQuery(text);
    if (parsed.text === "") {
      hits = [];
      error = null;
      loading = false;
      return;
    }

    loading = true;
    const timer = setTimeout(async () => {
      try {
        const found = await treeSearch(profileId, database, parsed.text, { showSystemSchemas: false });
        hits = filterHits(parsed, found);
        error = null;
      } catch (caught) {
        error = describeError(caught);
        hits = [];
      } finally {
        loading = false;
      }
    }, 200);

    return () => clearTimeout(timer);
  });

  function onkeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        onclose();
        break;
      case "ArrowDown":
        event.preventDefault();
        cursor = hits.length === 0 ? 0 : (cursor + 1) % hits.length;
        break;
      case "ArrowUp":
        event.preventDefault();
        cursor = hits.length === 0 ? 0 : (cursor - 1 + hits.length) % hits.length;
        break;
      case "Enter":
        event.preventDefault();
        if (hits[cursor]) onpick(hits[cursor], event.shiftKey);
        break;
    }
  }
</script>

<!-- Cierra al hacer clic afuera, igual que la paleta y la búsqueda: no hay nada escrito que perder. -->
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
        placeholder={t("Insertar tabla, vista, función…")}
        aria-label={t("Buscar un objeto para insertarlo")}
      />
      <kbd class="shrink-0 text-[10px] muted">Esc</kbd>
    </div>

    <div class="min-h-0 flex-1 overflow-auto py-1">
      {#if error}
        <Alert tone="bad">{error}</Alert>
      {:else if query.trim() === ""}
        <p class="px-3 py-6 text-center text-xs muted">{prefixHelp}</p>
      {:else if hits.length === 0 && !loading}
        <Empty
          icon="search"
          title={t("Sin coincidencias en el servidor")}
          hint={t("No hay ningún objeto cuyo nombre contenga «{query}» en esa base.", { query })}
        />
      {:else}
        {#each hits as hit, index (`${hit.schema}.${hit.oid}`)}
          {@const look = lookOf(hit.kind)}
          <button
            class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm
                   {index === cursor ? 'bg-blue-500/10' : ''}"
            onclick={(event) => onpick(hit, event.shiftKey)}
            onmouseenter={() => (cursor = index)}
          >
            <Icon name={look.icon} size={12} class="shrink-0 {look.tone}" />
            <span class="min-w-0 flex-1 truncate">{hit.label}</span>
            <span class="min-w-0 shrink-[100] truncate text-xs muted">{hit.schema}</span>
          </button>
        {/each}
      {/if}
    </div>

    <div class="divider-t px-3 py-1.5 text-[11px] muted">
      <kbd class="kbd">Enter</kbd> {t("inserta el nombre")} · <kbd class="kbd">{t("Mayús")}</kbd>+<kbd class="kbd">Enter</kbd>
      {t("inserta un SELECT con todas las columnas")}
    </div>
  </div>
</div>
