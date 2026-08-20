<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade, scale } from "svelte/transition";
  import Icon from "./Icon.svelte";

  /**
   * El diálogo de toda la aplicación.
   *
   * Antes cada diálogo repetía su propio fondo y su propia caja, y ninguno cerraba con Escape:
   * eran quince ventanas parecidas con quince comportamientos distintos. Acá viven las cosas que uno
   * espera de cualquier ventana modal —Escape, la trampa de foco y la devolución del foco al
   * cerrar— para que ningún diálogo nuevo tenga que acordarse de implementarlas.
   *
   * Lo que **no** hace es cerrarse al hacer clic en el fondo: estos diálogos son formularios de
   * varios campos con vista previa del SQL, y un clic al costado tirando media hora de trabajo es
   * mucho peor que un clic de más en «Cancelar». Se cierra con sus botones o con Escape, que es
   * intención explícita y no se aprieta sin querer.
   */
  let {
    title,
    subtitle = undefined,
    size = "md",
    /** Mientras algo está en curso no se cierra sola: perder el diálogo a mitad de un guardado
     * dejaría al usuario sin saber si se aplicó. */
    busy = false,
    onclose,
    children,
    footer = undefined,
  }: {
    title: string;
    subtitle?: string;
    size?: "sm" | "md" | "lg" | "xl";
    busy?: boolean;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  const WIDTH = {
    sm: "max-w-sm",
    md: "max-w-lg",
    lg: "max-w-2xl",
    xl: "max-w-3xl",
  } as const;

  let panel = $state<HTMLDivElement | null>(null);

  function focusable(): HTMLElement[] {
    if (!panel) return [];
    return [
      ...panel.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ];
  }

  // Al abrir se lleva el foco adentro y al cerrar se devuelve a donde estaba: sin esto, cerrar un
  // diálogo dejaba el foco en el `<body>` y el teclado sin punto de partida.
  $effect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const first = panel?.querySelector<HTMLElement>("[data-autofocus]") ?? focusable()[0];
    (first ?? panel)?.focus();

    return () => previous?.focus?.();
  });

  /**
   * Escape se escucha en la ventana y no en el panel: el foco puede estar en el `body` —basta con
   * hacer clic en el fondo oscuro— y entonces la tecla nunca pasaría por acá. Si hubiera dos
   * diálogos encimados, solo cierra el de más arriba.
   */
  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape" || busy) return;
    const open = [...document.querySelectorAll('[role="dialog"]')];
    if (open.at(-1) !== panel) return;
    event.preventDefault();
    onclose();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;

    // Trampa de foco: el tabulador da la vuelta dentro del diálogo en vez de irse a la ventana de
    // atrás, que está tapada y no se puede usar.
    const items = focusable();
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;

    if (event.shiftKey && (active === first || active === panel)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-30 grid place-items-center bg-zinc-950/50 p-4 backdrop-blur-[2px]"
  transition:fade={{ duration: 120 }}
  onkeydown={onKeydown}
>
  <div
    bind:this={panel}
    class="card flex max-h-[85vh] w-full {WIDTH[size]} flex-col overflow-hidden shadow-2xl
           shadow-zinc-950/20 outline-none"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    transition:scale={{ duration: 140, start: 0.97 }}
  >
    <div class="divider-b flex items-start gap-3 px-5 py-3">
      <div class="min-w-0 flex-1">
        <h2 class="truncate text-base font-medium">{title}</h2>
        {#if subtitle}
          <p class="mt-0.5 truncate text-xs muted">{subtitle}</p>
        {/if}
      </div>
      <button
        class="btn btn-ghost btn-icon -mr-1 shrink-0"
        aria-label="Cerrar"
        disabled={busy}
        onclick={onclose}
      >
        <Icon name="close" size={13} />
      </button>
    </div>

    <div class="min-h-0 flex-1 overflow-auto px-5 py-4 text-sm">
      {@render children()}
    </div>

    {#if footer}
      <div class="divider-t flex flex-wrap items-center gap-2 px-5 py-3">
        {@render footer()}
      </div>
    {/if}
  </div>
</div>
