<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlEditor from "./SqlEditor.svelte";
  import { describeError, functionApply } from "./ipc";

  let {
    profileId,
    database,
    sql,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    /** Ya trae la sentencia completa: un esqueleto para dar de alta, o el DDL actual para editar. */
    sql: string;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // Copia editable, tomada una sola vez: el diálogo se crea de nuevo cada vez que se abre.
  let text = $state(untrack(() => sql));
  let error = $state<string | null>(null);
  let saving = $state(false);

  async function submit() {
    error = null;
    if (!text.trim()) {
      error = "Hace falta una sentencia CREATE FUNCTION o CREATE PROCEDURE.";
      return;
    }

    saving = true;
    try {
      await functionApply(profileId, text, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title="Función o procedimiento"
  subtitle="Se ejecuta tal cual está escrito, contra {database}"
  size="xl"
  busy={saving}
  {onclose}
>
  <p class="mb-2 text-xs muted">
    Si el texto dice <code class="kbd">CREATE OR REPLACE</code>, reemplaza la que ya existe; si dice
    <code class="kbd">CREATE</code> a secas, crea una nueva.
  </p>

  <!-- El cuerpo de una función es código: va en el editor de SQL, no en un cuadro de texto. -->
  <div class="h-[420px] overflow-hidden rounded-md border border-zinc-200 dark:border-zinc-800">
    <SqlEditor bind:value={text} />
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      Ejecutar
    </button>
  {/snippet}
</Modal>
