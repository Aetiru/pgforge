<script lang="ts">
  import { i18n, t } from "./i18n.svelte";
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

<Modal title={t("Tabla muy grande en producción")} size="sm" onclose={() => onchoose("cancel")}>
  <p class="text-sm text-zinc-600 dark:text-zinc-300">
    <b class="font-mono">{table}</b>
    {t("tiene unas {rows} filas y la consulta no tiene", { rows: rows.toLocaleString(i18n.current) })}
    <code class="font-mono">LIMIT</code>{t(". La aplicación solo trae las primeras que entran en el máximo elegido, pero el servidor puede tardar mucho en producirlas.")}
  </p>

  {#snippet footer()}
    <button class="btn" onclick={() => onchoose("cancel")}>{t("Cancelar")}</button>
    <button class="btn" onclick={() => onchoose("run")}>{t("Ejecutar igual")}</button>
    <span class="ml-auto"></span>
    <button class="btn btn-primary" data-autofocus onclick={() => onchoose("limit")}>
      {t("Ejecutar con LIMIT {limit}", { limit: limit.toLocaleString(i18n.current) })}
    </button>
  {/snippet}
</Modal>
