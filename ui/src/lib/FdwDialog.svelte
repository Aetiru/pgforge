<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import OptionsEditor, {
    deltaIsEmpty,
    rowsFrom,
    toDelta,
    toOptions,
    type OptionRow,
  } from "./OptionsEditor.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import { describeError, fdwApply, fdwPreview, type FdwChange, type FdwInfo } from "./ipc";

  let {
    profileId,
    database,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    existing: FdwInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let name = $state(untrack(() => existing?.name ?? ""));
  let handler = $state(untrack(() => existing?.handler ?? ""));
  let validator = $state(untrack(() => existing?.validator ?? ""));
  let rows = $state<OptionRow[]>(untrack(() => rowsFrom(existing?.options ?? [])));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function changes(): FdwChange[] {
    if (!existing) {
      return [
        {
          kind: "create",
          name: name.trim(),
          handler: handler.trim() || null,
          validator: validator.trim() || null,
          options: toOptions(rows),
        },
      ];
    }

    const nextHandler = handler.trim();
    const prevHandler = existing.handler ?? "";
    const nextValidator = validator.trim();
    const prevValidator = existing.validator ?? "";
    const options = toDelta(existing.options, rows);

    const handlerChanged = nextHandler !== prevHandler;
    const validatorChanged = nextValidator !== prevValidator;
    if (!handlerChanged && !validatorChanged && deltaIsEmpty(options)) return [];

    return [
      {
        kind: "alter",
        name: existing.name,
        handler: handlerChanged && nextHandler ? nextHandler : null,
        noHandler: handlerChanged && !nextHandler,
        validator: validatorChanged && nextValidator ? nextValidator : null,
        noValidator: validatorChanged && !nextValidator,
        options,
      },
    ];
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para el wrapper.";
    return null;
  }

  async function showPreview() {
    error = validate();
    if (error) return;
    try {
      const list = changes();
      const statements = await fdwPreview(list);
      preview = statements.map((statement) => statement.sql).join(";\n\n") || "Nada que aplicar.";
    } catch (e) {
      error = describeError(e);
    }
  }

  async function submit() {
    error = validate();
    if (error) return;
    const list = changes();
    if (list.length === 0) {
      onsaved();
      return;
    }
    saving = true;
    try {
      await fdwApply(profileId, list, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Wrapper ${existing.name}` : "Nuevo wrapper foráneo"}
  subtitle="FOREIGN DATA WRAPPER"
  busy={saving}
  {onclose}
>
  <label class="flex flex-col gap-1">
    <span class="label">Nombre</span>
    <input class="field" data-autofocus bind:value={name} disabled={existing !== null} />
  </label>

  <div class="mt-3 grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Handler (opcional)</span>
      <input class="field" bind:value={handler} placeholder="p. ej. postgres_fdw_handler" />
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Validator (opcional)</span>
      <input class="field" bind:value={validator} placeholder="p. ej. postgres_fdw_validator" />
    </label>
  </div>

  <div class="mt-3">
    <span class="label">Opciones</span>
    <div class="mt-1"><OptionsEditor bind:rows /></div>
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
