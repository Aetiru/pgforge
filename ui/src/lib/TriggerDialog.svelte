<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    EVENT_OPTIONS,
    LEVEL_OPTIONS,
    TIMING_OPTIONS,
    toggleEvent,
    triggerChanges,
    triggerForm,
    validateTrigger,
  } from "./trigger-form";
  import {
    describeError,
    triggerApply,
    triggerPreview,
    type TriggerEvent,
    type TriggerInfo,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    table,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    table: string;
    /** `null` da de alta; si no, reemplaza el trigger que llega acá (borra y crea de nuevo). */
    existing: TriggerInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // Copia editable, tomada una sola vez: el diálogo se crea de nuevo cada vez que se abre.
  let form = $state(untrack(() => triggerForm(existing, schema)));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  const toggle = (event: TriggerEvent) => (form.events = toggleEvent(form.events, event));
  const changes = () => triggerChanges(form, { schema, table }, existing);
  const validate = () => validateTrigger(form);

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await triggerPreview(changes());
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

    if (!(await confirmMutation(profileId, "Se va a modificar un disparador."))) return;

    saving = true;
    try {
      await triggerApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar el trigger ${existing.name}` : "Nuevo trigger"}
  subtitle="{schema}.{table}"
  busy={saving}
  {onclose}
>
  {#if existing}
    <p class="mb-3 text-xs muted">
      PostgreSQL no permite alterar un trigger: se borra y se vuelve a crear con lo que quede acá,
      todo dentro de la misma transacción.
    </p>
  {/if}

  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={form.name} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Momento</span>
      <select class="field" bind:value={form.timing}>
        {#each TIMING_OPTIONS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="mt-3">
    <span class="label">Eventos</span>
    <div class="mt-1 flex flex-wrap gap-3">
      {#each EVENT_OPTIONS as option (option.value)}
        <label class="check">
          <input
            type="checkbox"
            checked={form.events.includes(option.value)}
            onchange={() => toggle(option.value)}
          />
          {option.label}
        </label>
      {/each}
    </div>
  </div>

  <div class="mt-3 grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nivel</span>
      <select class="field" bind:value={form.level}>
        {#each LEVEL_OPTIONS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">WHEN (opcional)</span>
    <input class="field" bind:value={form.when} placeholder="condición SQL, p. ej. NEW.activo" />
  </label>

  <div class="mt-3 grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Esquema de la función</span>
      <input class="field" bind:value={form.functionSchema} />
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Función a ejecutar</span>
      <input class="field" bind:value={form.functionName} />
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
      {existing ? "Guardar" : "Crear trigger"}
    </button>
  {/snippet}
</Modal>
