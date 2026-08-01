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

<div style="padding-left: {level * 18}px">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="flex items-baseline gap-2 rounded px-2 py-1 text-sm hover:bg-neutral-100
           dark:hover:bg-neutral-800"
    role="button"
    tabindex="0"
    onclick={() => onselect?.(node.pid)}
  >
    <span
      class="rounded px-1 font-mono text-xs {isRoot
        ? 'bg-rose-100 text-rose-700 dark:bg-rose-900 dark:text-rose-200'
        : 'bg-neutral-200 text-neutral-600 dark:bg-neutral-700 dark:text-neutral-300'}"
    >
      {node.pid}
    </span>

    {#if backend}
      <span class="text-xs text-neutral-500">
        {backend.user ?? "?"}@{backend.database ?? "?"}
        {#if backend.state}· {backend.state}{/if}
        {#if backend.querySeconds !== null}· {duration(backend.querySeconds)}{/if}
      </span>
      <span class="min-w-0 flex-1 truncate font-mono text-xs" title={backend.query ?? ""}>
        {oneLine(backend.query, 160)}
      </span>
    {:else}
      <!-- El filtro puede estar ocultando la sesión, pero el bloqueo existe igual. -->
      <span class="text-xs text-neutral-400">sesión fuera del filtro actual</span>
    {/if}
  </div>

  {#each node.blocking as child (child.pid)}
    <Self node={child} level={level + 1} {onselect} />
  {/each}
</div>
