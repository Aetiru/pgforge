<script lang="ts">
  import Modal from "./Modal.svelte";

  /**
   * Un `SELECT` sin límite sobre una tabla enorme, en producción. Tres salidas con nombre: acotarlo,
   * ejecutarlo igual, o volver al editor. Acotar agrega un `LIMIT` a lo que se manda, no al texto
   * del editor.
   */
  let {
    table,
    rows,
    limit,
    onchoose,
  }: {
    table: string;
    /** Filas estimadas de la tabla. */
    rows: number;
    limit: number;
    onchoose: (choice: "limit" | "run" | "cancel") => void;
  } = $props();
</script>

<Modal title="Tabla muy grande en producción" size="sm" onclose={() => onchoose("cancel")}>
  <p class="text-sm text-zinc-600 dark:text-zinc-300">
    <b class="font-mono">{table}</b> tiene unas <b>{rows.toLocaleString("es")}</b> filas y la
    consulta no tiene <code class="font-mono">LIMIT</code>. La aplicación solo trae las primeras
    que entran en el máximo elegido, pero el servidor puede tardar mucho en producirlas.
  </p>

  {#snippet footer()}
    <button class="btn" onclick={() => onchoose("cancel")}>Cancelar</button>
    <button class="btn" onclick={() => onchoose("run")}>Ejecutar igual</button>
    <span class="ml-auto"></span>
    <button class="btn btn-primary" data-autofocus onclick={() => onchoose("limit")}>
      Ejecutar con LIMIT {limit.toLocaleString("es")}
    </button>
  {/snippet}
</Modal>
