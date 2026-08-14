<script lang="ts">
  /**
   * La tarjeta de una sección del panel de detalle.
   *
   * Todas tienen la misma forma —encabezado con título y un botón para crear, y abajo el estado de
   * la lectura— y la diferencia empieza recién en la tabla. Acá vive esa parte igual: sin esto,
   * cada sección repetía las mismas cuatro ramas de carga, error y vacío.
   */
  import type { Snippet } from "svelte";
  import Alert from "../Alert.svelte";

  let {
    title,
    /** El texto que acompaña al spinner mientras se lee, o `null` si ya terminó. */
    loading = null,
    error = null,
    /** Qué decir cuando la lectura salió bien y no trajo nada. */
    empty = null,
    actions,
    children,
    class: klass = "",
  }: {
    title: string;
    loading?: string | null;
    error?: string | null;
    empty?: string | null;
    actions?: Snippet;
    children?: Snippet;
    class?: string;
  } = $props();
</script>

<div class="card overflow-hidden {klass}">
  <div class="card-head">
    <span class="card-title">{title}</span>
    {@render actions?.()}
  </div>

  {#if loading}
    <p class="flex items-center gap-2 px-3 py-4 text-sm muted">
      <span class="spinner"></span>
      {loading}
    </p>
  {:else if error}
    <Alert tone="bad" box class="m-3">{error}</Alert>
  {:else if empty}
    <p class="px-3 py-4 text-sm muted">{empty}</p>
  {:else}
    {@render children?.()}
  {/if}
</div>
