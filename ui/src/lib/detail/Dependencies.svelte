<script lang="ts">
  /** De qué depende el objeto y qué se rompe si cambia: claves foráneas y vistas. */
  import Icon, { type IconName } from "../Icon.svelte";
  import type { Dependency } from "../ipc";
  import Card from "./Card.svelte";

  let {
    dependencies,
    loading = false,
    error = null,
  }: {
    dependencies: Dependency[] | null;
    loading?: boolean;
    error?: string | null;
  } = $props();

  const uses = $derived(dependencies?.filter((item) => item.direction === "uses") ?? []);
  const usedBy = $derived(dependencies?.filter((item) => item.direction === "usedBy") ?? []);

  const ICON: Record<string, IconName> = {
    tabla: "table",
    "tabla particionada": "partitioned",
    vista: "view",
    "vista materializada": "matview",
    "clave foránea": "constraint",
  };
</script>

{#snippet group(title: string, hint: string, items: Dependency[])}
  {#if items.length > 0}
    <section class="flex flex-col gap-1">
      <h3 class="px-3 pt-2 text-xs font-semibold tracking-wide uppercase muted" title={hint}>
        {title} <span class="font-normal">({items.length})</span>
      </h3>
      <ul>
        {#each items as item, index (index)}
          <li class="flex items-baseline gap-2 px-3 py-1 text-sm">
            <Icon name={ICON[item.kind] ?? "table"} size={12} class="shrink-0 translate-y-0.5 muted" />
            <span class="font-medium whitespace-nowrap">{item.schema}.{item.name}</span>
            <span class="tag tag-neutral shrink-0">{item.kind}</span>
            {#if item.detail}
              <span class="min-w-0 flex-1 truncate font-mono text-xs muted" title={item.detail}>
                {item.detail}
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

<Card
  title="Dependencias"
  loading={loading ? "Leyendo dependencias…" : null}
  {error}
  empty={dependencies && dependencies.length === 0 ? "Ningún objeto depende de este ni él de otro." : null}
>
  {#if dependencies}
    {@render group("Depende de", "Lo que este objeto necesita para existir", uses)}
    {@render group("Lo usan", "Lo que deja de andar o de poder crearse si este objeto cambia", usedBy)}
  {/if}
</Card>
