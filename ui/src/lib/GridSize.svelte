<script lang="ts">
  import { t } from "./i18n.svelte";
  import { gridZoom } from "./grid.svelte";

  /**
   * Los tres botones del tamaño de la grilla, copia de `FontSize.svelte` para `gridZoom`.
   *
   * `Ctrl` + rueda sobre la grilla no se descubre solo, y con los botones se puede ajustar sin haber
   * puesto el mouse justo encima de una fila.
   */
</script>

<div class="flex items-center" title={t("Tamaño de la grilla — Ctrl + rueda")}>
  <button
    class="btn btn-ghost btn-icon btn-sm text-[11px]"
    disabled={!gridZoom.canShrink}
    aria-label={t("Achicar la grilla")}
    title={t("Achicar")}
    onclick={() => gridZoom.smaller()}
  >
    A−
  </button>
  <button
    class="btn btn-ghost btn-sm px-1 text-[10px] tabular-nums muted"
    aria-label={t("Volver al tamaño normal")}
    title={t("Volver al tamaño normal")}
    onclick={() => gridZoom.reset()}
  >
    {gridZoom.size}
  </button>
  <button
    class="btn btn-ghost btn-icon btn-sm text-sm"
    disabled={!gridZoom.canGrow}
    aria-label={t("Agrandar la grilla")}
    title={t("Agrandar")}
    onclick={() => gridZoom.bigger()}
  >
    A+
  </button>
</div>
