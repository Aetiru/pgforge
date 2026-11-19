<script lang="ts">
  import Modal from "./Modal.svelte";

  /**
   * Hay una transacción abierta y la acción que se pidió la afecta: cerrar la pestaña o encender el
   * autocommit. Tres salidas con nombre —confirmar, revertir, o seguir como estaba— en vez de un
   * «Aceptar» que decide por el usuario qué pasa con lo que no confirmó.
   */
  let {
    title,
    message,
    keepLabel = undefined,
    busy = false,
    onchoose,
    onclose,
  }: {
    title: string;
    message: string;
    /** Tercera salida: seguir con la acción sin tocar la transacción (solo donde tiene sentido). */
    keepLabel?: string;
    busy?: boolean;
    onchoose: (choice: "commit" | "rollback" | "keep") => void;
    onclose: () => void;
  } = $props();
</script>

<Modal {title} size="sm" {busy} {onclose}>
  <p class="text-sm text-zinc-600 dark:text-zinc-300">{message}</p>

  {#snippet footer()}
    <button class="btn" disabled={busy} onclick={onclose}>Cancelar</button>
    {#if keepLabel}
      <button class="btn" disabled={busy} onclick={() => onchoose("keep")}>{keepLabel}</button>
    {/if}
    <span class="ml-auto"></span>
    <button class="btn btn-danger" disabled={busy} onclick={() => onchoose("rollback")}>
      Rollback
    </button>
    <button
      class="btn btn-primary"
      disabled={busy}
      data-autofocus
      onclick={() => onchoose("commit")}
    >
      Commit
    </button>
  {/snippet}
</Modal>
