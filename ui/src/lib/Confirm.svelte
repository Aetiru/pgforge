<script lang="ts">
  import type { Snippet } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";

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
    onconfirm: () => void;
    onclose: () => void;
    /** Opciones extra del comando: van debajo del texto y arriba del error. */
    children?: Snippet;
  } = $props();
</script>

<Modal {title} size="sm" {busy} {onclose}>
  <p class="text-sm text-zinc-600 dark:text-zinc-300">{message}</p>

  {#if children}
    <div class="mt-3">{@render children()}</div>
  {/if}

  {#if error}
    <Alert class="mt-3" box tone="bad">{error}</Alert>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" disabled={busy} onclick={onclose}>Cancelar</button>
    <button
      class="btn {danger ? 'btn-danger' : 'btn-primary'}"
      disabled={busy}
      data-autofocus
      onclick={onconfirm}
    >
      {#if busy}<span class="spinner"></span>{/if}
      {confirmLabel}
    </button>
  {/snippet}
</Modal>
