<script lang="ts">
  import { t } from "./i18n.svelte";
  /**
   * El esqueleto de lo que está por llegar: barras con la forma de las filas, en lugar de una rueda
   * girando en un hueco. Mantiene el tamaño del lugar, así que no salta cuando llegan los datos, y se
   * siente más rápido aunque el tiempo sea el mismo. El texto es lo que lee el lector de pantalla.
   */
  let {
    rows = 5,
    label = t("Cargando…"),
    compact = false,
  }: {
    rows?: number;
    /** Lo que anuncia el lector de pantalla: lo visible son solo barras. */
    label?: string;
    /** Filas más bajas, para paneles angostos. */
    compact?: boolean;
  } = $props();

  // Anchos fijos y no al azar: un esqueleto que cambia de forma en cada dibujo parece una falla.
  const WIDTHS = [88, 64, 76, 52, 82, 70, 58];
</script>

<div class="flex flex-col {compact ? 'gap-2' : 'gap-3'} p-4" role="status" aria-label={label} aria-busy="true">
  {#each Array.from({ length: rows }, (_, index) => index) as row (row)}
    <div class="flex items-center gap-3">
      <div class="skeleton size-4 shrink-0 rounded"></div>
      <div class="skeleton {compact ? 'h-2.5' : 'h-3'}" style="width: {WIDTHS[row % WIDTHS.length]}%"></div>
      <div class="skeleton ml-auto hidden h-3 w-14 shrink-0 sm:block"></div>
    </div>
  {/each}
  <span class="sr-only">{label}</span>
</div>
