<script lang="ts">
  import type { Snippet } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { typedMatches } from "./statement-risk";

  /**
   * La confirmación de una acción que no se deshace.
   *
   * Todas dicen lo mismo con la misma forma: qué va a pasar, las opciones que cambian cómo pasa
   * (CASCADE, CONCURRENTLY) y el error si falla, sin cerrar el diálogo, que es donde el usuario está
   * mirando.
   */
  let {
    title,
    message,
    confirmLabel = "Aceptar",
    danger = true,
    busy = false,
    error = null,
    requireText = undefined,
    onconfirm,
    onclose,
    children = undefined,
  }: {
    title: string;
    message: string;
    confirmLabel?: string;
    /** Casi todas cortan o borran algo; las que no, piden el botón en azul. */
    danger?: boolean;
    busy?: boolean;
    error?: string | null;
    /**
     * Si viene, el botón queda apagado hasta escribir exactamente este texto. Es la confirmación
     * escalonada de producción: apretar «Aceptar» de memoria no alcanza.
     */
    requireText?: string;
    onconfirm: () => void;
    onclose: () => void;
    /** Opciones extra del comando: van debajo del texto y arriba del error. */
    children?: Snippet;
  } = $props();

  let typed = $state("");
  const allowed = $derived(requireText === undefined || typedMatches(typed, requireText));
</script>

<Modal {title} size="sm" {busy} {onclose}>
  <p class="text-sm text-zinc-600 dark:text-zinc-300">{message}</p>

  {#if children}
    <div class="mt-3">{@render children()}</div>
  {/if}

  {#if requireText !== undefined}
    <label class="mt-3 block">
      <span class="label">Para continuar, escribí <strong class="select-text">{requireText}</strong></span>
      <input
        class="field mt-1 w-full font-mono"
        bind:value={typed}
        autocomplete="off"
        spellcheck="false"
        aria-label="Nombre del servidor"
        data-autofocus
        onkeydown={(event) => {
          if (event.key === "Enter" && allowed && !busy) onconfirm();
        }}
      />
    </label>
  {/if}

  {#if error}
    <Alert class="mt-3" box tone="bad">{error}</Alert>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" disabled={busy} onclick={onclose}>Cancelar</button>
    <button
      class="btn {danger ? 'btn-danger' : 'btn-primary'}"
      disabled={busy || !allowed}
      data-autofocus={requireText === undefined ? true : undefined}
      onclick={onconfirm}
    >
      {#if busy}<span class="spinner"></span>{/if}
      {confirmLabel}
    </button>
  {/snippet}
</Modal>
