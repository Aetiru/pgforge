<script lang="ts">
  import Icon from "./Icon.svelte";

  let { server, database, title }: { server: string; database: string; title: string } = $props();

  const parts = $derived([server, database, title].filter((part) => part !== ""));
</script>

<!--
  Dónde está parado lo que se ve: servidor › base › objeto. Con varias conexiones abiertas la
  pestaña sola no lo dice (el nombre del servidor se recorta), y averiguarlo pedía pasar el mouse
  por encima. Es solo lectura: la navegación sigue siendo el árbol.
-->
{#if parts.length > 1}
  <nav
    class="divider-b flex h-6 shrink-0 items-center gap-1 overflow-hidden px-3 text-[11px] muted"
    aria-label="Ubicación"
  >
    {#each parts as part, index (index)}
      {#if index > 0}
        <Icon name="chevron" size={9} class="shrink-0 opacity-60" />
      {/if}
      <span
        class="truncate {index === parts.length - 1
          ? 'font-medium text-zinc-700 dark:text-zinc-200'
          : ''}"
        title={part}
      >
        {part}
      </span>
    {/each}
  </nav>
{/if}
