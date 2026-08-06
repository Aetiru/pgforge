<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    commentApply,
    commentPreview,
    describeError,
    type CommentChange,
    type CommentTarget,
  } from "./ipc";

  let {
    profileId,
    database,
    target,
    label,
    current,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    target: CommentTarget;
    /** Cómo se llama el objeto, para el título. */
    label: string;
    current: string | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let text = $state(untrack(() => current ?? ""));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function changes(): CommentChange[] {
    // En blanco borra: un comentario vacío se ve igual que ninguno pero ocupa lugar en el catálogo.
    return [{ target, comment: text.trim() === "" ? null : text }];
  }

  async function showPreview() {
    error = null;
    try {
      const statements = await commentPreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n");
    } catch (e) {
      error = describeError(e);
    }
  }

  async function submit() {
    error = null;
    if (!(await confirmMutation(profileId, "Se va a cambiar un comentario."))) return;

    saving = true;
    try {
      await commentApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title="Comentario" subtitle={label} busy={saving} {onclose}>
  <label class="flex flex-col gap-1">
    <span class="label">Texto</span>
    <textarea class="field h-28 resize-none" data-autofocus bind:value={text}></textarea>
  </label>
  <p class="mt-1 text-xs muted">Dejalo vacío para borrar el comentario.</p>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      Guardar
    </button>
  {/snippet}
</Modal>
