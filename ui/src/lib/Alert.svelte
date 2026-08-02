<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  /**
   * Un aviso: el error que cortó una operación, la advertencia de que algo es de solo lectura, o la
   * confirmación de que una acción salió bien. Los tres se ven igual salvo por el color y el ícono,
   * así que la diferencia se elige con una palabra y no copiando ocho clases.
   */
  let {
    tone = "bad",
    /** En bloque, con borde alrededor, para adentro de un formulario; si no, franja de ancho completo. */
    box = false,
    onclose = undefined,
    class: className = "",
    children,
  }: {
    tone?: "bad" | "warn" | "ok";
    box?: boolean;
    /** Si se pasa, el aviso se puede descartar. */
    onclose?: () => void;
    class?: string;
    children: Snippet;
  } = $props();

  const ICON: Record<"bad" | "warn" | "ok", IconName> = {
    bad: "warn",
    warn: "warn",
    ok: "check",
  };
</script>

<div class="{box ? 'alert-box' : 'alert'} alert-{tone} {className}" role={tone === "bad" ? "alert" : undefined}>
  <Icon name={ICON[tone]} size={14} class="mt-0.5 shrink-0 opacity-80" />
  <span class="min-w-0 flex-1 break-words">{@render children()}</span>
  {#if onclose}
    <button
      class="btn btn-ghost btn-icon -my-0.5 -mr-1 size-6 shrink-0 text-current opacity-70
             hover:bg-black/5 hover:text-current dark:hover:bg-white/10"
      aria-label="Descartar el aviso"
      onclick={onclose}
    >
      <Icon name="close" size={12} />
    </button>
  {/if}
</div>
