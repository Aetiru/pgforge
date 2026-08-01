<script lang="ts">
  import Self from "./PlanTree.svelte";
  import { count, decimal } from "./format";
  import type { PlanNode } from "./ipc";

  let {
    node,
    level = 0,
    /** Tiempo propio del nodo más caro de todo el plan, para dimensionar la barra. */
    worst = 0,
  }: {
    node: PlanNode;
    level?: number;
    worst?: number;
  } = $props();

  const share = $derived(worst > 0 && node.selfMs !== null ? node.selfMs / worst : 0);
  /** Solo se destaca lo que de verdad pesa: pintar todo de rojo no señala nada. */
  const heavy = $derived(share >= 0.3);

  const subtitle = $derived.by(() => {
    if (node.index && node.relation) return `${node.relation} vía ${node.index}`;
    return node.relation ?? node.index ?? "";
  });
</script>

<div style="padding-left: {level * 16}px">
  <div class="relative rounded px-2 py-1 hover:bg-zinc-100 dark:hover:bg-zinc-800/70">
    <!-- La barra vive detrás del texto: ocupa lugar sin robarle ancho a lo que hay que leer. -->
    {#if share > 0}
      <div
        class="absolute inset-y-0 left-0 rounded {heavy
          ? 'bg-rose-500/15'
          : 'bg-blue-500/10'}"
        style="width: {Math.max(2, share * 100)}%"
      ></div>
    {/if}

    <div class="relative flex items-baseline gap-2 text-sm">
      <span class="font-medium">{node.nodeType}</span>
      {#if subtitle}
        <span class="truncate text-xs muted">{subtitle}</span>
      {/if}

      <span class="ml-auto flex shrink-0 items-baseline gap-2 text-xs tabular-nums">
        {#if node.selfMs !== null}
          <span class={heavy ? "font-medium text-rose-600 dark:text-rose-400" : "muted"}>
            {decimal(node.selfMs, 2)} ms
          </span>
        {/if}
        <span class="muted">costo {decimal(node.totalCost, 2)}</span>
      </span>
    </div>

    <div class="relative flex flex-wrap items-baseline gap-x-3 text-xs muted">
      {#if node.actualRows !== null}
        <span class={node.misestimated ? "text-amber-600 dark:text-amber-400" : ""}>
          filas {count(node.planRows)} estimadas / {count(node.actualRows)} reales
          {#if node.misestimated}⚠{/if}
        </span>
      {:else}
        <span>filas {count(node.planRows)} estimadas</span>
      {/if}

      {#if node.loops !== null && node.loops > 1}
        <span>{count(node.loops)} vueltas</span>
      {/if}

      {#if node.rowsRemoved !== null && node.rowsRemoved > 0}
        <span>descartó {count(node.rowsRemoved)}</span>
      {/if}

      {#if node.sharedReadBlocks !== null && node.sharedReadBlocks > 0}
        <span>{count(node.sharedReadBlocks)} bloques del disco</span>
      {/if}
    </div>

    {#if node.condition}
      <div class="relative truncate font-mono text-xs muted" title={node.condition}>
        {node.condition}
      </div>
    {/if}
  </div>

  {#each node.children as child, index (index)}
    <Self node={child} level={level + 1} {worst} />
  {/each}
</div>
