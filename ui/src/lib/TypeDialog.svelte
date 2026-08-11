<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    compositeChanges,
    droppedLabels,
    enumChanges,
    type FieldRow,
    type LabelRow,
  } from "./type-form";
  import {
    describeError,
    typeApply,
    typeInfo,
    typePreview,
    type TypeChange,
    type TypeField,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    composite,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    schema: string;
    /** Al dar de alta decide qué se crea. Al editar lo manda el servidor. */
    composite: boolean;
    existing: { oid: number; name: string } | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let isComposite = $state(untrack(() => composite));
  let name = $state(untrack(() => existing?.name ?? ""));
  let labels = $state<LabelRow[]>([{ original: "", value: "" }]);
  let fields = $state<FieldRow[]>([{ original: "", name: "", dataType: "text" }]);

  /** Lo que hay en el servidor, para poder comparar contra lo escrito. */
  let beforeLabels = $state<string[]>([]);
  let beforeFields = $state<TypeField[]>([]);

  let loading = $state(untrack(() => existing !== null));
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  $effect(() => {
    if (!existing) return;
    loading = true;
    typeInfo(profileId, existing.oid, database)
      .then((info) => {
        isComposite = info.kind === "composite";
        beforeLabels = info.labels;
        beforeFields = info.fields;
        labels = info.labels.map((value) => ({ original: value, value }));
        fields = info.fields.map((field) => ({
          original: field.name,
          name: field.name,
          dataType: field.dataType,
        }));
        if (labels.length === 0) labels = [{ original: "", value: "" }];
        if (fields.length === 0) fields = [{ original: "", name: "", dataType: "text" }];
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loading = false));
  });

  const perdidos = $derived(existing && !isComposite ? droppedLabels(beforeLabels, labels) : []);

  function changes(): TypeChange[] {
    const target = existing ? existing.name : name.trim();

    if (!existing) {
      if (isComposite) {
        return [
          {
            kind: "createComposite",
            schema,
            name: target,
            fields: fields
              .filter((row) => row.name.trim() !== "" && row.dataType.trim() !== "")
              .map((row) => ({
                name: row.name.trim(),
                dataType: row.dataType.trim(),
                collation: null,
              })),
          },
        ];
      }
      return [
        {
          kind: "createEnum",
          schema,
          name: target,
          labels: labels.map((row) => row.value.trim()).filter((value) => value !== ""),
        },
      ];
    }

    return isComposite
      ? compositeChanges(schema, target, beforeFields, fields)
      : enumChanges(schema, target, beforeLabels, labels);
  }

  function validate(): string | null {
    if (!existing && !name.trim()) return "Poné un nombre para el tipo.";

    if (isComposite) {
      const usables = fields.filter(
        (row) => row.name.trim() !== "" && row.dataType.trim() !== "",
      );
      if (usables.length === 0) return "Un tipo compuesto necesita al menos un campo.";
      const nombres = usables.map((row) => row.name.trim());
      if (new Set(nombres).size !== nombres.length) return "Hay campos con el mismo nombre.";
    } else {
      const valores = labels.map((row) => row.value.trim()).filter((value) => value !== "");
      if (valores.length === 0) return "Una enumeración necesita al menos un valor.";
      if (new Set(valores).size !== valores.length) return "Hay valores repetidos.";
    }

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
      const statements = await typePreview(changes());
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

    if (!(await confirmMutation(profileId, "Se va a modificar un tipo."))) return;

    saving = true;
    try {
      await typeApply(profileId, changes(), database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar ${existing.name}` : isComposite ? "Nuevo tipo compuesto" : "Nueva enumeración"}
  subtitle={schema}
  size="lg"
  busy={saving}
  {onclose}
>
  {#if loading}
    <p class="flex items-center justify-center gap-2 py-8 text-sm muted">
      <span class="spinner"></span>
      Leyendo la definición…
    </p>
  {:else}
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={name} disabled={existing !== null} />
    </label>

    {#if isComposite}
      <div class="mt-4 flex flex-col gap-2">
        <span class="label">Campos</span>
        {#each fields as row, index (index)}
          <div class="flex items-center gap-2">
            <input
              class="field flex-1"
              placeholder="nombre"
              bind:value={row.name}
              disabled={row.original !== ""}
            />
            <input class="field flex-1" placeholder="tipo" bind:value={row.dataType} />
            <button
              class="btn btn-icon"
              title="Quitar"
              aria-label="Quitar campo"
              onclick={() => (fields = fields.filter((_, i) => i !== index))}
            >
              <Icon name="trash" size={12} />
            </button>
          </div>
        {/each}
        <button
          class="btn btn-sm self-start"
          onclick={() => (fields = [...fields, { original: "", name: "", dataType: "text" }])}
        >
          <Icon name="plus" size={11} />
          Agregar campo
        </button>
        {#if existing}
          <p class="text-xs muted">
            El nombre de un campo que ya existe no se puede editar acá: cambiarlo y cambiarle el tipo
            a la vez haría imposible distinguir un renombre de un borrado más un alta.
          </p>
        {/if}
      </div>
    {:else}
      <div class="mt-4 flex flex-col gap-2">
        <span class="label">Valores</span>
        {#each labels as row, index (index)}
          <div class="flex items-center gap-2">
            <input class="field flex-1" placeholder="valor" bind:value={row.value} />
            <button
              class="btn btn-icon"
              title="Quitar"
              aria-label="Quitar valor"
              onclick={() => (labels = labels.filter((_, i) => i !== index))}
            >
              <Icon name="trash" size={12} />
            </button>
          </div>
        {/each}
        <button
          class="btn btn-sm self-start"
          onclick={() => (labels = [...labels, { original: "", value: "" }])}
        >
          <Icon name="plus" size={11} />
          Agregar valor
        </button>
      </div>

      {#if perdidos.length > 0}
        <Alert tone="warn" box class="mt-3">
          PostgreSQL no puede quitar un valor de una enumeración: {perdidos.join(", ")}
          {perdidos.length === 1 ? "se va a conservar" : "se van a conservar"}. Para sacarlo hay que
          recrear el tipo y todas las columnas que lo usan.
        </Alert>
      {/if}
    {/if}
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving || loading}>
      Ver SQL
    </button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving || loading}>
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Crear"}
    </button>
  {/snippet}
</Modal>
