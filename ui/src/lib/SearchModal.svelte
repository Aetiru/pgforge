<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { lookOf } from "./badges";
  import { describeError, objectDdl, treeSearch, type SearchHit, type TreeNode } from "./ipc";
  import { openQuery } from "./query.svelte";
  import { WITH_ROWS } from "./tree-actions";
  import { filterHits, parseQuery, PREFIX_HELP } from "./tree-query";

  /**
   * Búsqueda rápida contra el catálogo del servidor (Ctrl+Mayús+P), separada de la paleta de
   * comandos (Ctrl+K): esa solo ofrece lo que el árbol ya trajo, y acá la pregunta va al servidor
   * —mismo `tree_search` que usa la caja de búsqueda del árbol, con el mismo vocabulario de
   * prefijos (`PREFIX_HELP`)—. Elegir un resultado no lo revela en el árbol: abre una pestaña de
   * consulta nueva, siempre, con el `SELECT *` o el DDL según el tipo de objeto.
   */
  let {
    profileId,
    database,
    onclose,
  }: {
    profileId: string;
    database: string;
    onclose: () => void;
  } = $props();

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let cursor = $state(0);
  let input = $state<HTMLInputElement | null>(null);
  let opening = $state(false);

  $effect(() => {
    input?.focus();
  });

  // Como la caja de la paleta, pero contra el servidor: no hay nada cargado antes para filtrar
  // local, así que cada tecla pide de nuevo. El rebote evita mandar una consulta por letra.
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
        const found = await treeSearch(profileId, database, parsed.text, {
          showSystemSchemas: false,
        });
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

  /** El nombre completo, como se escribe en una consulta —misma idea que `qualifiedNameOf`. */
  function qualifiedOf(hit: SearchHit): string {
    return `${hit.schema}.${hit.label}`;
  }

  /**
   * Un `TreeNode` de mentira para pedirle el DDL a `objectDdl`: `id` y `hasChildren` son del árbol
   * y no los usa el generador de DDL, así que un valor cualquiera alcanza.
   */
  function asNode(hit: SearchHit): TreeNode {
    return {
      id: "",
      hasChildren: false,
      kind: hit.kind,
      label: hit.label,
      database: hit.database,
      schema: hit.schema,
      oid: hit.oid,
    };
  }

  /**
   * Elegir un resultado abre una pestaña de consulta, siempre —a diferencia de la búsqueda del
   * árbol, que revela el nodo—: es lo que pidió esta búsqueda, pensada para llegar al SQL y no al
   * árbol. Tabla, vista o vista materializada nace con un `SELECT *`; el resto —función,
   * procedimiento, secuencia, tipo— con su DDL, que es lo único que tiene sentido leer.
   */
  async function pick(hit: SearchHit | undefined) {
    if (!hit || opening) return;
    opening = true;
    try {
      const tab = await openQuery(profileId, hit.database, qualifiedOf(hit));
      onclose();

      if (typeof hit.kind === "string" && WITH_ROWS.includes(hit.kind)) {
        tab.sql = `SELECT * FROM ${qualifiedOf(hit)}`;
        return;
      }

      try {
        const ddl = await objectDdl(profileId, asNode(hit));
        tab.sql = ddl.sql;
      } catch (caught) {
        tab.log("error", describeError(caught));
      }
    } finally {
      opening = false;
    }
  }

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
        void pick(hits[cursor]);
        break;
    }
  }
</script>

<!-- Cerrar al hacer clic afuera, igual que la paleta: es una búsqueda, no un formulario con algo
     escrito que perder. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40 flex justify-center bg-zinc-950/40 p-4 pt-[12vh] backdrop-blur-[2px]"
  onclick={(event) => event.target === event.currentTarget && onclose()}
  onkeydown={onkeydown}
>
  <div class="card flex max-h-[70vh] w-full max-w-xl flex-col overflow-hidden shadow-2xl">
    <div class="divider-b flex items-center gap-2 px-3 py-2">
      <Icon name="search" size={14} class="shrink-0 text-zinc-400" />
      <input
        bind:this={input}
        bind:value={query}
        class="w-full bg-transparent text-sm outline-none placeholder:text-zinc-400"
        placeholder="Buscar tablas, vistas, funciones, procedimientos, tipos…"
      />
      <kbd class="shrink-0 text-[10px] muted">Esc</kbd>
    </div>

    <div class="min-h-0 flex-1 overflow-auto py-1">
      {#if error}
        <Alert tone="bad">{error}</Alert>
      {:else if query.trim() === ""}
        <p class="px-3 py-6 text-center text-xs muted">{PREFIX_HELP}</p>
      {:else if hits.length === 0 && !loading}
        <Empty
          icon="search"
          title="Sin coincidencias en el servidor"
          hint="No hay ninguna tabla, vista, función, procedimiento ni tipo cuyo nombre contenga «{query}» en esa base."
        />
      {:else}
        {#each hits as hit, index (`${hit.schema}.${hit.oid}`)}
          {@const look = lookOf(hit.kind)}
          <button
            class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm
                   {index === cursor ? 'bg-blue-500/10' : ''}"
            onclick={() => pick(hit)}
            onmouseenter={() => (cursor = index)}
          >
            <Icon name={look.icon} size={12} class="shrink-0 {look.tone}" />
            <span class="min-w-0 flex-1 truncate">{hit.label}</span>
            <span class="min-w-0 shrink-[100] truncate text-xs muted">{hit.schema}</span>
            {#if hit.detail}
              <span class="ml-auto shrink-0 pl-1 text-xs muted">{hit.detail}</span>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>
