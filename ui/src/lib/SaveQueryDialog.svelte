<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { describeError, savedSave } from "./ipc";
  import type { QueryTab } from "./query.svelte";

  let {
    tab,
    onclose,
    onsaved,
  }: {
    tab: QueryTab;
    onclose: () => void;
    /** Para que el panel de guardadas se entere sin tener que releer solo. */
    onsaved?: () => void;
  } = $props();

  // Copia editable tomada una sola vez, como en el resto de los formularios.
  let name = $state(untrack(() => tab.savedName ?? tab.title));
  /**
   * Si la pestaña salió de una consulta guardada, lo normal es reescribir esa. Desmarcarlo es
   * «guardar como»: la misma consulta con otro nombre, dejando la original donde estaba.
   */
  let overwrite = $state(untrack(() => tab.savedId !== null));
  let error = $state<string | null>(null);
  let busy = $state(false);

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre: es con lo que la vas a buscar después.";
    if (!tab.sql.trim()) return "El editor está vacío, no hay nada que guardar.";
    return null;
  }

  async function submit() {
    error = validate();
    if (error) return;

    busy = true;
    try {
      const saved = await savedSave({
        id: overwrite ? tab.savedId : null,
        name: name.trim(),
        sql: tab.sql,
        profileId: tab.profileId,
        database: tab.database,
      });

      tab.savedId = saved.id;
      tab.savedName = saved.name;
      tab.title = saved.name;
      onsaved?.();
      onclose();
    } catch (e) {
      // El nombre repetido llega como conflicto desde el núcleo, que no pisa lo que había.
      error = describeError(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal
  title={tab.savedId === null ? "Guardar la consulta" : "Guardar los cambios"}
  subtitle="{tab.database} · {tab.sql.trim().split('\n').length} líneas"
  size="sm"
  {busy}
  {onclose}
>
  <label class="flex flex-col gap-1">
    <span class="label">Nombre</span>
    <input
      class="field"
      data-autofocus
      bind:value={name}
      onkeydown={(event) => {
        if (event.key === "Enter") submit();
      }}
    />
  </label>

  {#if tab.savedId !== null}
    <label class="check mt-3">
      <input type="checkbox" bind:checked={overwrite} />
      Reescribir «{tab.savedName}»
    </label>
    <p class="mt-1 text-xs muted">
      Sin marcar queda una consulta nueva y la de antes se conserva tal como estaba.
    </p>
  {/if}

  <p class="mt-3 text-xs muted">
    Se guarda en esta máquina, junto con el historial, no en el servidor. Queda anotado contra qué
    base se escribió, pero se puede abrir contra cualquier otra.
  </p>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" disabled={busy} onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" disabled={busy} onclick={submit}>
      {#if busy}<span class="spinner"></span>{:else}<Icon name="save" size={12} />{/if}
      Guardar
    </button>
  {/snippet}
</Modal>
