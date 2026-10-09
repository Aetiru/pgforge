<script lang="ts">
  import Icon from "./Icon.svelte";
  import Self from "./PlanTree.svelte";
  import { count, decimal } from "./format";
  import type { PlanNode } from "./ipc";
  import { heatLevel, weightOf, type PlanHeat } from "./plan-heat";

  let {
    node,
    level = 0,
    /** Qué pesa el nodo más caro de todo el plan, para dimensionar la barra y elegir el color. */
    heat,
  }: {
    node: PlanNode;
    level?: number;
    heat: PlanHeat;
  } = $props();

  const weight = $derived(weightOf(node, heat.measure));
  const share = $derived(heat.worst > 0 ? weight / heat.worst : 0);
  const warmth = $derived(heatLevel(share));
  /** Qué parte del total es este nodo; con costo estimado se dice «del costo», no del tiempo. */
  const percent = $derived(heat.total > 0 ? (weight / heat.total) * 100 : 0);
  /** Solo se destaca lo que de verdad pesa: pintar todo de rojo no señala nada. */
  const heavy = $derived(warmth >= 3);

  /**
   * Cinco escalones de frío a caliente (`plan-heat.ts`). El azul del primero es el mismo de la
   * interfaz: lo que pesa poco se funde con el fondo y lo que pesa mucho es lo único que cambia de
   * matiz.
   */
  const HEAT_BG = [
    "",
    "bg-blue-500/10",
    "bg-amber-500/15",
    "bg-orange-500/20",
    "bg-rose-500/25",
  ];

  const subtitle = $derived.by(() => {
    if (node.index && node.relation) return `${node.relation} vía ${node.index}`;
    return node.relation ?? node.index ?? "";
  });

  /** Una rama se puede plegar: en un plan de cuarenta nodos, el que importa suele estar arriba. */
  let open = $state(true);
</script>

<div class="relative" style="padding-left: {level > 0 ? 16 : 0}px">
  {#if level > 0}
    <span class="absolute inset-y-0 left-[7px] w-px bg-zinc-200 dark:bg-zinc-700"></span>
  {/if}

  <div class="relative rounded px-2 py-1 hover:bg-zinc-100 dark:hover:bg-zinc-700/70">
    <!-- La barra vive detrás del texto: ocupa lugar sin robarle ancho a lo que hay que leer. -->
    {#if share > 0}
      <div
        class="absolute inset-y-0 left-0 rounded {HEAT_BG[warmth]}"
        style="width: {Math.max(2, share * 100)}%"
      ></div>
    {/if}

    <div class="relative flex items-baseline gap-2 text-sm">
      {#if node.children.length > 0}
        <button
          class="grid size-4 shrink-0 translate-y-0.5 place-items-center rounded text-zinc-400
                 hover:text-zinc-900 dark:hover:text-zinc-100"
          aria-label={open ? "Plegar la rama" : "Desplegar la rama"}
          onclick={() => (open = !open)}
        >
          <Icon name="chevron" size={11} class="transition-transform {open ? 'rotate-90' : ''}" />
        </button>
      {:else}
        <span class="size-4 shrink-0"></span>
      {/if}

      <span class="font-medium">{node.nodeType}</span>
      {#if subtitle}
        <span class="truncate text-xs muted">{subtitle}</span>
      {/if}
      {#if !open && node.children.length > 0}
        <span class="tag tag-neutral">
          +{node.children.length}
          {node.children.length === 1 ? "rama" : "ramas"}
        </span>
      {/if}

      <span class="ml-auto flex shrink-0 items-baseline gap-2 text-xs tabular-nums">
        {#if node.selfMs !== null}
          <span class={heavy ? "font-medium text-rose-600 dark:text-rose-400" : "muted"}>
            {decimal(node.selfMs, 2)} ms
          </span>
        {/if}
        {#if warmth > 0}
          <span
            class={heavy ? "font-medium text-rose-600 dark:text-rose-400" : "muted"}
            title={heat.measure === "ms"
              ? "Parte del tiempo total que gasta este nodo, sin contar a sus hijos"
              : "Parte del costo total que estima el planificador para este nodo, sin contar a sus hijos"}
          >
            {decimal(percent, percent < 10 ? 1 : 0)} %
          </span>
        {/if}
        <span class="muted">costo {decimal(node.totalCost, 2)}</span>
      </span>
    </div>

    <div class="relative flex flex-wrap items-baseline gap-x-3 pl-6 text-xs muted">
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
      <div class="relative truncate pl-6 font-mono text-xs muted" title={node.condition}>
        {node.condition}
      </div>
    {/if}
  </div>

  {#if open}
    {#each node.children as child, index (index)}
      <Self node={child} level={level + 1} {heat} />
    {/each}
  {/if}
</div>
