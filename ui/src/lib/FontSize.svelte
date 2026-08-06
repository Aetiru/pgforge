<script lang="ts">
  import { sqlFont } from "./editor.svelte";

  /**
   * Los tres botones del tamaño de letra del SQL.
   *
   * Existen porque los atajos (`Ctrl +`, `Ctrl -`, `Ctrl 0`) no se descubren solos, y el editor no
   * es el único lugar donde se lee SQL: el DDL de un objeto no tiene foco de teclado y sin estos
   * botones no habría forma de agrandarlo.
   */
</script>

<div class="flex items-center" title="Tamaño de letra del SQL — Ctrl +, Ctrl - y Ctrl 0">
  <button
    class="btn btn-ghost btn-icon btn-sm text-[11px]"
    disabled={!sqlFont.canShrink}
    aria-label="Achicar la letra del SQL"
    title="Achicar (Ctrl -)"
    onclick={() => sqlFont.smaller()}
  >
    A−
  </button>
  <button
    class="btn btn-ghost btn-sm px-1 text-[10px] tabular-nums muted"
    aria-label="Volver al tamaño normal"
    title="Volver al tamaño normal (Ctrl 0)"
    onclick={() => sqlFont.reset()}
  >
    {sqlFont.size}
  </button>
  <button
    class="btn btn-ghost btn-icon btn-sm text-sm"
    disabled={!sqlFont.canGrow}
    aria-label="Agrandar la letra del SQL"
    title="Agrandar (Ctrl +)"
    onclick={() => sqlFont.bigger()}
  >
    A+
  </button>
</div>
