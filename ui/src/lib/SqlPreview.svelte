<script lang="ts">
  /**
   * El SQL que un diálogo va a ejecutar.
   *
   * Todos los formularios de DDL ofrecen verlo antes de aplicar: es la única forma de que el
   * usuario aprenda qué hace la herramienta en lugar de tener que confiar en ella.
   */
  let { sql }: { sql: string } = $props();
</script>

<pre
  class="mt-3 max-h-48 overflow-auto rounded-md border border-zinc-200 bg-zinc-50 p-2 font-mono
         text-xs whitespace-pre-wrap select-text dark:border-zinc-800 dark:bg-zinc-800/50">{sql}</pre>
