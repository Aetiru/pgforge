<script lang="ts">
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { describeError } from "./ipc";
  import { updates } from "./update.svelte";

  let {
    current,
    onclose,
  }: {
    /** La versión que está corriendo, para que el diálogo diga de dónde a dónde se va. */
    current: string;
    onclose: () => void;
  } = $props();

  let error = $state<string | null>(null);
  let busy = $state(false);

  const release = $derived(updates.release);

  /** La fecha de publicación, en el formato local. Sin hora: no aporta nada acá. */
  const published = $derived.by(() => {
    const value = release?.publishedAt;
    if (!value) return null;
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? null : date.toLocaleDateString();
  });

  async function open() {
    error = null;
    busy = true;
    try {
      await updates.open();
    } catch (e) {
      error = describeError(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if release}
  <Modal
    title="pgforge {release.version}"
    subtitle="Estás en la {current}{published ? ` · publicada el ${published}` : ''}"
    {busy}
    {onclose}
  >
    <!-- Las notas van como texto y no interpretadas: es Markdown escrito a mano en la release, y
         sumar un intérprete por un cartel que aparece cada varias semanas no se paga. -->
    {#if release.notes.trim()}
      <pre
        class="max-h-72 overflow-auto rounded border border-zinc-200 bg-zinc-50 p-3 text-xs
               whitespace-pre-wrap select-text dark:border-zinc-800 dark:bg-zinc-900">{release.notes.trim()}</pre>
    {:else}
      <p class="text-sm muted">Esta versión salió sin notas.</p>
    {/if}

    <p class="mt-3 text-xs muted">
      pgforge no se actualiza solo: el botón abre la página de la release en el navegador, donde está
      el instalador de cada sistema. «Ahora no» silencia esta versión; la siguiente vuelve a avisar.
    </p>

    {#if error}
      <Alert tone="bad" box class="mt-3">{error}</Alert>
    {/if}

    {#snippet footer()}
      <button class="btn ml-auto" disabled={busy} onclick={() => updates.dismiss()}>Ahora no</button>
      <button class="btn btn-primary" data-autofocus disabled={busy} onclick={open}>
        {#if busy}<span class="spinner"></span>{:else}<Icon name="download" size={12} />{/if}
        Ver la descarga
      </button>
    {/snippet}
  </Modal>
{/if}
