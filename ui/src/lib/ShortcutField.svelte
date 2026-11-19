<script lang="ts">
  import { comboFromEvent, formatCombo } from "./shortcuts";
  import { MAC } from "./shortcuts.svelte";

  /**
   * El campo de «Atajo» de una abreviatura: apretar el botón y después la combinación. Retroceso o
   * Supr la quitan y Escape cancela.
   *
   * Mismo patrón que `ShortcutsDialog`: mientras espera, escucha en fase de captura y corta la
   * propagación, porque ni el atajo que se está por pisar ni el `Escape` del diálogo deben
   * enterarse de la tecla.
   */
  let {
    value = $bindable(null),
    onchange,
  }: { value?: string | null; onchange?: (combo: string | null) => void } = $props();

  let capturing = $state(false);

  $effect(() => {
    if (!capturing) return;

    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
      const bare = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
      if (event.key === "Escape") {
        capturing = false;
        return;
      }
      if (bare && (event.key === "Backspace" || event.key === "Delete")) {
        value = null;
        onchange?.(null);
        capturing = false;
        return;
      }
      const combo = comboFromEvent(event);
      if (combo === null) return; // solo un modificador: se espera la tecla
      value = combo;
      onchange?.(combo);
      capturing = false;
    };

    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });
</script>

<div class="flex items-center gap-2">
  <button
    type="button"
    class="btn btn-sm min-w-32 font-mono"
    aria-pressed={capturing}
    title="Apretá y después la combinación. Retroceso la quita; Escape cancela."
    onclick={() => (capturing = !capturing)}
  >
    {capturing ? "Apretá la combinación…" : value ? formatCombo(value, MAC) : "Sin atajo"}
  </button>
  {#if value && !capturing}
    <button
      type="button"
      class="btn btn-ghost btn-sm"
      onclick={() => {
        value = null;
        onchange?.(null);
      }}>Quitar</button
    >
  {/if}
</div>
