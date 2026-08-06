<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import { describeError, schemaApply, schemaPreview, type SchemaChange } from "./ipc";

  let {
    profileId,
    database,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    /** `null` da de alta; si no, renombra o cambia el dueño del que llega acá. */
    existing: { name: string; owner: string } | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let name = $state(untrack(() => existing?.name ?? ""));
  let owner = $state(untrack(() => existing?.owner ?? ""));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function changes(): SchemaChange[] {
    if (!existing) {
      return [
        {
          kind: "createSchema",
          name: name.trim(),
          authorization: owner.trim() === "" ? null : owner.trim(),
          ifNotExists: false,
        },
      ];
    }

    const list: SchemaChange[] = [];
    if (name.trim() !== existing.name) {
      list.push({ kind: "renameSchema", name: existing.name, newName: name.trim() });
    }
    if (owner.trim() !== "" && owner.trim() !== existing.owner) {
      // Sobre el nombre nuevo: el renombre ya corrió en la misma transacción.
      list.push({ kind: "setSchemaOwner", name: name.trim(), owner: owner.trim() });
    }
    return list;
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para el esquema.";
    if (existing && changes().length === 0) return "No hay nada que cambiar.";
    return null;
  }

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await schemaPreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n");
    } catch (e) {
      error = describeError(e);
    }
  }

  async function submit() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }

    if (!(await confirmMutation(profileId, "Se va a modificar un esquema."))) return;

    saving = true;
    try {
      await schemaApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar ${existing.name}` : "Nuevo esquema"}
  subtitle={database}
  busy={saving}
  {onclose}
>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={name} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Dueño</span>
      <input class="field" bind:value={owner} placeholder="el rol conectado" />
    </label>
  </div>

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
      {existing ? "Guardar" : "Crear"}
    </button>
  {/snippet}
</Modal>
