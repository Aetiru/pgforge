<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { allContainers, defaultOpen, flatten, type JsonKind, type JsonRow } from "./json-tree";

  /**
   * El árbol plegable de un JSON/JSONB. La lógica (qué filas se ven, cómo se escribe cada ruta) es
   * `json-tree.ts`; acá solo se dibuja la lista aplanada, sin un componente por nodo.
   */
  let { value }: { value: unknown } = $props();

  let open = $state(untrack(() => defaultOpen(value)));
  let copied = $state<string | null>(null);

  const rows = $derived(flatten(value, open));

  const TONE: Record<JsonKind, string> = {
    object: "muted",
    array: "muted",
    string: "text-emerald-700 dark:text-emerald-400",
    number: "text-blue-700 dark:text-blue-400",
    boolean: "text-amber-700 dark:text-amber-400",
    null: "italic muted",
  };

  function toggle(path: string) {
    // `open` se reasigna entero: un `Set` mutado no avisa a Svelte.
    const next = new Set(open);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    open = next;
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      copied = path;
      setTimeout(() => (copied = copied === path ? null : copied), 1200);
    } catch {
      copied = null;
    }
  }

  const isRow = (row: (typeof rows)[number]): row is JsonRow => "kind" in row;
</script>

<div class="mb-2 flex items-center gap-2 text-xs">
  <button class="btn btn-sm" onclick={() => (open = allContainers(value))}>Desplegar todo</button>
  <button class="btn btn-sm" onclick={() => (open = new Set(["$"]))}>Plegar todo</button>
  <span class="ml-auto muted">
    {copied ? "Ruta copiada" : "Clic en una ruta para copiarla"}
  </span>
</div>

<div
  class="max-h-[60vh] overflow-auto rounded bg-zinc-50 p-2 font-mono text-xs select-text
         dark:bg-zinc-800/60"
  role="tree"
>
  {#each rows as row (row.path)}
    {#if isRow(row)}
      <div
        class="group flex items-center gap-1 whitespace-nowrap"
        style="padding-left: {row.depth * 14}px"
        role="treeitem"
        aria-selected="false"
        aria-expanded={row.size > 0 ? row.open : undefined}
      >
        {#if row.size > 0}
          <button
            class="shrink-0"
            aria-label={row.open ? "Plegar" : "Desplegar"}
            onclick={() => toggle(row.path)}
          >
            <Icon name="chevron" size={11} class={row.open ? "rotate-90" : ""} />
          </button>
        {:else}
          <span class="inline-block w-[11px] shrink-0"></span>
        {/if}
        {#if row.key !== null}<span>{row.key}</span><span class="muted">:</span>{/if}
        <span class={TONE[row.kind]}>{row.text}</span>
        <button
          class="ml-2 hidden text-[10px] muted hover:underline group-hover:inline"
          title="Copiar la ruta (jsonpath)"
          onclick={() => copyPath(row.path)}
        >
          {row.path}
        </button>
      </div>
    {:else}
      <div class="muted" style="padding-left: {row.depth * 14 + 15}px">… {row.more} más</div>
    {/if}
  {/each}
</div>
