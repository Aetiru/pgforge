<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    COMMON_TYPES,
    IDENTITY_OPTIONS,
    columnChanges,
    columnForm,
    validateColumn,
  } from "./column-form";
  import { ddlApply, ddlPreview, describeError, type TableColumn } from "./ipc";

  let {
    profileId,
    database,
    schema,
    table,
    column,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    table: string;
    /** `null` da de alta una columna nueva; si no, edita la que llega acá. */
    column: TableColumn | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // Copia editable, tomada una sola vez: en modo edición es lo que se compara contra el original
  // para mandar solo lo que de verdad cambió.
  let form = $state(untrack(() => columnForm(column)));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  const typeChanged = $derived(column !== null && form.typeName.trim() !== column.typeName);
  const validate = () => validateColumn(form);
  const changes = () => columnChanges(form, { schema, table }, column);

  const pending = $derived(changes().length);

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await ddlPreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n") || "Nada que aplicar.";
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
    const list = changes();
    if (list.length === 0) {
      onclose();
      return;
    }

    if (!(await confirmMutation(profileId, "Se van a modificar columnas de la tabla."))) return;

    saving = true;
    try {
      await ddlApply(profileId, list, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={column ? `Editar la columna ${column.name}` : "Nueva columna"}
  subtitle="{schema}.{table}"
  busy={saving}
  {onclose}
>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={form.name} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Tipo</span>
      <input class="field" list="pgforge-column-types" bind:value={form.typeName} />
    </label>

    {#if column && typeChanged}
      <label class="col-span-2 flex flex-col gap-1">
        <span class="label">USING (opcional, solo si el cambio de tipo no es implícito)</span>
        <input class="field" placeholder={`${column.name}::${form.typeName}`} bind:value={form.using} />
      </label>
    {/if}

    {#if !column}
      <label class="flex flex-col gap-1">
        <span class="label">Identidad</span>
        <select class="field" bind:value={form.identity}>
          {#each IDENTITY_OPTIONS as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
    {/if}

    <label class="flex flex-col gap-1">
      <span class="label">Default</span>
      <input
        class="field"
        disabled={!column && form.identity !== ""}
        bind:value={form.default}
        placeholder="expresión SQL, p. ej. now()"
      />
    </label>

    <label class="check col-span-2">
      <input type="checkbox" bind:checked={form.notNull} />
      NOT NULL
    </label>

    <datalist id="pgforge-column-types">
      {#each COMMON_TYPES as type (type)}
        <option value={type}></option>
      {/each}
    </datalist>
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    {#if column}
      <span class="text-xs muted">
        {pending > 0 ? `${pending} cambio${pending === 1 ? "" : "s"}` : "sin cambios"}
      </span>
    {/if}
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button
      class="btn btn-primary"
      onclick={submit}
      disabled={saving || (column !== null && pending === 0)}
    >
      {#if saving}<span class="spinner"></span>{/if}
      {column ? "Guardar" : "Agregar columna"}
    </button>
  {/snippet}
</Modal>
