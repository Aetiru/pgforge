<script lang="ts">
  import { untrack } from "svelte";
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

<div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
  <div
    class="card flex max-h-[85vh] w-full max-w-3xl flex-col shadow-xl"
    role="dialog"
    aria-modal="true"
    aria-label="Función o procedimiento"
  >
    <h2 class="divider-b px-5 py-3 text-base font-medium">Función o procedimiento</h2>

    <div class="min-h-0 flex-1 overflow-auto px-5 py-4 text-sm">
      <p class="mb-2 text-xs muted">
        Se ejecuta tal cual: si el texto dice <code>CREATE OR REPLACE</code>, reemplaza la que ya
        existe; si dice <code>CREATE</code> a secas, crea una nueva.
      </p>
      <textarea
        class="field h-full min-h-[320px] w-full font-mono text-xs"
        bind:value={text}
        spellcheck="false"
      ></textarea>

      {#if error}
        <p class="mt-3 text-sm text-rose-600 dark:text-rose-400">{error}</p>
      {/if}
    </div>

    <div class="divider-t flex items-center gap-2 px-5 py-3">
      <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
      <button class="btn btn-primary" onclick={submit} disabled={saving}>Ejecutar</button>
    </div>
  </div>
</div>
