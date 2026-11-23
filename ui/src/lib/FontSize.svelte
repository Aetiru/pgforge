<script lang="ts">
  import { t } from "./i18n.svelte";
  import { sqlDensity, sqlFont } from "./editor.svelte";

  /**
   * Los tres botones del tamaño de letra del SQL.
   *
   * Existen porque los atajos (`Ctrl +`, `Ctrl -`, `Ctrl 0`) no se descubren solos, y el editor no
   * es el único lugar donde se lee SQL: el DDL de un objeto no tiene foco de teclado y sin estos
   * botones no habría forma de agrandarlo.
   */
</script>

<div class="flex items-center" title={t("Tamaño de letra del SQL — Ctrl +, Ctrl - y Ctrl 0")}>
  <button
    class="btn btn-ghost btn-icon btn-sm text-[11px]"
    disabled={!sqlFont.canShrink}
    aria-label={t("Achicar la letra del SQL")}
    title={t("Achicar (Ctrl -)")}
    onclick={() => sqlFont.smaller()}
  >
    A−
  </button>
  <button
    class="btn btn-ghost btn-sm px-1 text-[10px] tabular-nums muted"
    aria-label={t("Volver al tamaño normal")}
    title={t("Volver al tamaño normal (Ctrl 0)")}
    onclick={() => sqlFont.reset()}
  >
    {sqlFont.size}
  </button>
  <button
    class="btn btn-ghost btn-icon btn-sm text-sm"
    disabled={!sqlFont.canGrow}
    aria-label={t("Agrandar la letra del SQL")}
    title={t("Agrandar (Ctrl +)")}
    onclick={() => sqlFont.bigger()}
  >
    A+
  </button>
  <button
    class="btn btn-ghost btn-icon btn-sm text-[11px]"
    aria-pressed={sqlDensity.mode === "compact"}
    aria-label={t("Interlineado del SQL")}
    title={sqlDensity.mode === "compact"
      ? t("Interlineado compacto: apretá para volver al cómodo")
      : t("Interlineado cómodo: apretá para ver más líneas (compacto)")}
    onclick={() => sqlDensity.toggle()}
  >
    ↕
  </button>
</div>
