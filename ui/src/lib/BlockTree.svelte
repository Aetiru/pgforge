<script lang="ts">
  import Self from "./BlockTree.svelte";
  import { duration, oneLine } from "./format";
  import { monitor } from "./monitor.svelte";
  import type { BlockNode } from "./ipc";

  let {
    node,
    level = 0,
    onselect,
  }: {
    node: BlockNode;
    level?: number;
    onselect?: (pid: number) => void;
  } = $props();

  const backend = $derived(monitor.backendOf(node.pid));
  /** La raíz es la que bloquea sin estar bloqueada: es la sesión sobre la que hay que actuar. */
  const isRoot = $derived(level === 0);
</script>

<div class="relative" style="padding-left: {level > 0 ? 18 : 0}px">
  <!-- La línea que baja de la sesión de arriba: es la que hace ver la cadena como una cadena. -->
  {#if level > 0}
    <span class="absolute inset-y-0 left-[8px] w-px bg-zinc-200 dark:bg-zinc-800"></span>
  {/if}

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="flex items-baseline gap-2 rounded px-2 py-1 text-sm hover:bg-zinc-100
           dark:hover:bg-zinc-800"
    role="button"
    tabindex="0"
    title="Ver esta sesión en la lista"
    onclick={() => onselect?.(node.pid)}
  >
    <span class="tag font-mono {isRoot ? 'tag-bad' : 'tag-neutral'}">
      {node.pid}
    </span>

    {#if isRoot}
      <span class="tag tag-warn shrink-0">la que bloquea</span>
    {/if}

    {#if backend}
      <span class="shrink-0 text-xs muted">
        {backend.user ?? "?"}@{backend.database ?? "?"}
        {#if backend.state}· {backend.state}{/if}
        {#if backend.querySeconds !== null}· {duration(backend.querySeconds)}{/if}
      </span>
      <span class="min-w-0 flex-1 truncate font-mono text-xs" title={backend.query ?? ""}>
        {oneLine(backend.query, 160)}
      </span>
    {:else}
      <!-- El filtro puede estar ocultando la sesión, pero el bloqueo existe igual. -->
      <span class="text-xs text-zinc-400">sesión fuera del filtro actual</span>
    {/if}
  </div>

  {#each node.blocking as child (child.pid)}
    <Self node={child} level={level + 1} {onselect} />
  {/each}
</div>
