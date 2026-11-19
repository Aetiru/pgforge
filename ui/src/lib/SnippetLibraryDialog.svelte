<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import Sql from "./Sql.svelte";
  import { checkAbbreviation } from "./snippet-check";
  import { LIBRARY, libraryCategories, type LibraryItem } from "./snippet-library";
  import { filterSnippets } from "./snippet-list";
  import { snippets } from "./snippets.svelte";

  /**
   * La biblioteca de plantillas listas. Agregar una la copia a la lista del usuario, donde se
   * edita como cualquier otra: no queda ningún vínculo con la biblioteca, así que cambiarla acá
   * después no toca lo que ya se agregó.
   */
  let { onclose }: { onclose: () => void } = $props();

  const ALL = "Todas";
  const categories = [ALL, ...libraryCategories()];

  let category = $state(ALL);
  let query = $state("");
  let selectedId = $state<string | null>(LIBRARY[0]?.id ?? null);
  /** La abreviatura con la que se agregará: se puede cambiar si choca con una existente. */
  let abbreviation = $state(LIBRARY[0]?.abbreviation ?? "");
  let busy = $state(false);
  let added = $state<string | null>(null);

  const installed = $derived(
    new Set(snippets.items.map((item) => item.abbreviation.trim().toLowerCase())),
  );

  // La biblioteca tiene la forma de una abreviatura a los efectos de buscar: `SnippetItem` pide lo mismo.
  const shown = $derived(
    filterSnippets(
      LIBRARY.filter((entry) => category === ALL || entry.category === category),
      query,
    ),
  );
  const selected = $derived(LIBRARY.find((entry) => entry.id === selectedId) ?? null);

  function select(entry: LibraryItem) {
    selectedId = entry.id;
    abbreviation = entry.abbreviation;
    added = null;
  }

  // Elegir otra plantilla repone su abreviatura; el efecto vive en `select` y no acá porque un
  // `$effect` sobre `selected` pisaría lo que el usuario está renombrando.

  const check = $derived(checkAbbreviation(abbreviation, snippets.items, null));
  /** Ya está agregada con **su** abreviatura de fábrica: no se ofrece de nuevo sin renombrar. */
  const already = $derived(selected !== null && installed.has(selected.abbreviation.toLowerCase()) && abbreviation.trim().toLowerCase() === selected.abbreviation.toLowerCase());

  async function add() {
    if (!selected || check.errors.length > 0) return;
    busy = true;
    const ok = await snippets.save({
      abbreviation: abbreviation.trim(),
      body: selected.body,
      description: selected.description,
      category: selected.category,
      scope: { profileIds: [], databases: [] },
      shortcut: null,
    });
    busy = false;
    if (ok) added = abbreviation.trim();
  }
</script>

<Modal
  title="Biblioteca de plantillas"
  subtitle="Elegí una y agregala a tus abreviaturas: después se edita como cualquier otra."
  size="xl"
  {busy}
  {onclose}
>
  {#if snippets.error}
    <Alert tone="bad" box class="mb-3" onclose={() => (snippets.error = null)}>
      {snippets.error}
    </Alert>
  {/if}

  <div class="grid min-h-[22rem] grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)] gap-4">
    <div class="flex min-w-0 flex-col gap-2">
      <input
        class="field w-full"
        data-autofocus
        placeholder="Buscar plantillas…"
        aria-label="Buscar plantillas"
        bind:value={query}
        onkeydown={(event) => event.key === "Escape" && query !== "" && (event.stopPropagation(), (query = ""))}
      />
      <div class="seg flex-wrap" role="tablist">
        {#each categories as name (name)}
          <button
            class="seg-item"
            role="tab"
            aria-selected={category === name}
            onclick={() => (category = name)}>{name}</button
          >
        {/each}
      </div>
      <ul class="max-h-[22rem] min-h-0 flex-1 space-y-1 overflow-y-auto pr-1">
        {#each shown as entry (entry.id)}
          <li>
            <button
              class="w-full rounded border px-2 py-1.5 text-left text-sm
                     {selectedId === entry.id
                ? 'border-blue-500 bg-blue-50 dark:bg-blue-950/40'
                : 'border-zinc-200 hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800'}"
              onclick={() => select(entry)}
            >
              <span class="font-mono">{entry.abbreviation}</span>
              <span class="ml-2">{entry.description}</span>
              {#if installed.has(entry.abbreviation.toLowerCase())}
                <span class="tag tag-ok ml-2">Agregada</span>
              {/if}
            </button>
          </li>
        {:else}
          <li><Empty icon="search" title="Ninguna plantilla coincide" /></li>
        {/each}
      </ul>
    </div>

    <div class="min-w-0">
      {#if selected}
        <h3 class="text-sm font-semibold">{selected.description}</h3>
        <p class="text-xs muted">{selected.category}</p>
        <div class="my-3 max-h-60 overflow-auto rounded border border-zinc-200 p-2 dark:border-zinc-700">
          <Sql code={selected.body} />
        </div>

        <label class="field">
          <span class="label">Abreviatura con la que se agrega</span>
          <input
            class="w-40 font-mono"
            bind:value={abbreviation}
            spellcheck="false"
            aria-label="Abreviatura"
          />
        </label>
        {#each check.errors as message (message)}
          <p class="mt-1 text-xs text-rose-600 dark:text-rose-400">{message}</p>
        {/each}
        {#each check.warnings as message (message)}
          <p class="mt-1 text-xs text-amber-600 dark:text-amber-400">{message}</p>
        {/each}

        {#if added}
          <Alert tone="ok" box class="mt-3">
            Agregada como «{added}». Escribila en el editor y apretá Tab.
          </Alert>
        {/if}
      {:else}
        <Empty icon="sql" title="Elegí una plantilla" />
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
    <button
      class="btn btn-primary"
      disabled={busy || !selected || check.errors.length > 0 || already}
      title={already ? "Ya está agregada: cambiale la abreviatura para agregarla otra vez" : undefined}
      onclick={add}
    >
      <Icon name="plus" size={12} />
      Agregar
    </button>
  {/snippet}
</Modal>
