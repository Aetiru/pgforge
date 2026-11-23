<script lang="ts">
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { t } from "./i18n.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import { COMMON_TYPES, IDENTITY_OPTIONS } from "./column-form";
  import { blankColumn, tableChange, validateTable, type DraftColumn } from "./table-form";
  import { ddlApply, ddlPreview, describeError, type Identity } from "./ipc";

  let {
    profileId,
    database,
    schema,
    onclose,
    oncreated,
  }: {
    profileId: string;
    database: string;
    schema: string;
    onclose: () => void;
    oncreated: () => void;
  } = $props();

  let name = $state("");
  let columns = $state<DraftColumn[]>([blankColumn()]);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function addColumn() {
    columns = [...columns, blankColumn()];
  }

  function removeColumn(key: string) {
    columns = columns.filter((column) => column.key !== key);
  }

  // Las etiquetas viven en `column-form.ts` (en español): se traducen al dibujar.
  function identityLabel(value: string): string {
    if (value === "always") return t("Siempre");
    if (value === "byDefault") return t("Por defecto");
    return t("Ninguna");
  }

  const change = () => tableChange(schema, name, columns);
  const validate = () => validateTable(name, columns);

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await ddlPreview([change()]);
      preview = statements.map((statement) => statement.sql).join(";\n\n");
    } catch (e) {
      error = describeError(e);
    }
  }

  /**
   * El SQL que se va a ejecutar, para que la confirmación de producción sepa si trae algo
   * destructivo. Se pide solo si hace falta y reutiliza la vista previa si ya está a la vista.
   */
  async function sqlToApply(): Promise<string> {
    if (preview) return preview;
    const statements = await ddlPreview([change()]);
    return statements.map((statement) => statement.sql).join(";\n\n");
  }

  async function submit() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }

    if (!(await confirmMutation(profileId, t("Se va a modificar la estructura de la tabla."), sqlToApply))) return;

    saving = true;
    try {
      await ddlApply(profileId, [change()], database);
      oncreated();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={t("Nueva tabla")} subtitle={t("en el esquema {schema}", { schema })} size="lg" busy={saving} {onclose}>
  <label class="flex flex-col gap-1">
    <span class="label">{t("Nombre")}</span>
    <input class="field" data-autofocus bind:value={name} placeholder={t("clientes")} />
  </label>

  <div class="mt-4 flex items-center justify-between">
    <span class="label">{t("Columnas")}</span>
    <button class="btn btn-sm" onclick={addColumn}>
      <Icon name="plus" size={11} />
      {t("Columna")}
    </button>
  </div>

  <!-- Los rótulos van una sola vez arriba y no repetidos en cada fila: con seis columnas, el
       formulario se leía como una lista de etiquetas y no como una tabla. -->
  <div class="mt-2 grid grid-cols-[8rem_10rem_8rem_9rem_auto_1.75rem] gap-1.5 px-2 text-[11px] muted">
    <span>{t("Nombre")}</span>
    <span>{t("Tipo")}</span>
    <span>{t("Identidad")}</span>
    <span>Default</span>
    <span></span>
    <span></span>
  </div>

  <div class="mt-1 flex flex-col gap-1.5">
    {#each columns as column (column.key)}
      <div
        class="grid grid-cols-[8rem_10rem_8rem_9rem_auto_1.75rem] items-center gap-1.5 rounded-md
               border border-zinc-200 p-1.5 dark:border-zinc-700"
      >
        <input class="field" placeholder={t("nombre")} bind:value={column.name} />
        <input
          class="field"
          placeholder={t("tipo")}
          list="pgforge-common-types"
          bind:value={column.typeName}
        />
        <select
          class="field"
          value={column.identity ?? ""}
          onchange={(event) => {
            const value = event.currentTarget.value;
            column.identity = value === "" ? null : (value as Identity);
            if (column.identity) column.default = null;
          }}
        >
          {#each IDENTITY_OPTIONS as option (option.value)}
            <option value={option.value}>{identityLabel(option.value)}</option>
          {/each}
        </select>
        <input
          class="field"
          placeholder="default"
          disabled={column.identity !== null}
          value={column.default ?? ""}
          oninput={(event) => (column.default = event.currentTarget.value || null)}
        />
        <label class="check justify-self-start">
          <input type="checkbox" bind:checked={column.notNull} />
          NOT NULL
        </label>
        <button
          class="btn btn-ghost btn-icon size-7"
          title={t("Quitar la columna")}
          aria-label={t("Quitar la columna")}
          disabled={columns.length === 1}
          onclick={() => removeColumn(column.key)}
        >
          <Icon name="close" size={11} />
        </button>
      </div>
    {/each}
  </div>

  <datalist id="pgforge-common-types">
    {#each COMMON_TYPES as type (type)}
      <option value={type}></option>
    {/each}
  </datalist>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} {profileId} {database} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>{t("Ver SQL")}</button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>{t("Cancelar")}</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      {t("Crear tabla")}
    </button>
  {/snippet}
</Modal>
