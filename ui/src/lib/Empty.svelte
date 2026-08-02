<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  /**
   * El estado vacío.
   *
   * Una pantalla en blanco no distingue «todavía no elegiste nada» de «esto está roto». Acá siempre
   * hay un ícono, una frase que dice qué pasa y —cuando existe— el siguiente paso, que es lo único
   * que el usuario quiere en ese momento.
   */
  let {
    icon = "compass",
    title,
    hint = undefined,
    children = undefined,
  }: {
    icon?: IconName;
    title: string;
    hint?: string;
    /** La acción que resuelve el vacío, si hay una sola obvia. */
    children?: Snippet;
  } = $props();
</script>

<div class="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
  <div
    class="grid size-12 place-items-center rounded-full bg-zinc-100 text-zinc-400
           dark:bg-zinc-900 dark:text-zinc-600"
  >
    <Icon name={icon} size={22} />
  </div>
  <div class="max-w-sm">
    <p class="text-sm font-medium text-zinc-700 dark:text-zinc-300">{title}</p>
    {#if hint}
      <p class="mt-1 text-xs muted">{hint}</p>
    {/if}
  </div>
  {#if children}
    <div class="flex flex-wrap items-center justify-center gap-2">{@render children()}</div>
  {/if}
</div>
