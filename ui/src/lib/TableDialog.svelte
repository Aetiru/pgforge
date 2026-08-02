<script lang="ts">
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    ddlApply,
    ddlPreview,
    describeError,
    type ColumnDef,
    type Identity,
    type TableChange,
  } from "./ipc";

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

  /** Tipos comunes, solo como sugerencia: el que valida un tipo de verdad es el servidor. */
  const COMMON_TYPES = [
    "bigint",
    "integer",
    "smallint",
    "text",
    "varchar(255)",
    "numeric(12,2)",
    "boolean",
    "timestamptz",
    "date",
    "uuid",
    "jsonb",
    "bytea",
  ];

  const IDENTITY_OPTIONS: { value: Identity | ""; label: string }[] = [
    { value: "", label: "Ninguna" },
    { value: "always", label: "Siempre" },
    { value: "byDefault", label: "Por defecto" },
  ];

  /** Una fila del formulario. `key` es solo para que Svelte identifique la fila; no viaja al núcleo. */
  interface DraftColumn extends ColumnDef {
    key: string;
  }

  function blankColumn(): DraftColumn {
    return {
      key: crypto.randomUUID(),
      name: "",
      typeName: "",
      notNull: false,
      default: null,
      identity: null,
    };
  }

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

  function change(): TableChange {
    return {
      kind: "createTable",
      schema,
      name: name.trim(),
      columns: columns.map(({ key: _key, ...column }) => column),
    };
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para la tabla.";
    if (columns.length === 0) return "Una tabla necesita al menos una columna.";
    for (const column of columns) {
      if (!column.name.trim()) return "Todas las columnas necesitan un nombre.";
      if (!column.typeName.trim()) return `La columna ${column.name} necesita un tipo.`;
    }
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
      const statements = await ddlPreview([change()]);
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

<Modal title="Nueva tabla" subtitle="en el esquema {schema}" size="lg" busy={saving} {onclose}>
  <label class="flex flex-col gap-1">
    <span class="label">Nombre</span>
    <input class="field" data-autofocus bind:value={name} placeholder="clientes" />
  </label>

  <div class="mt-4 flex items-center justify-between">
    <span class="label">Columnas</span>
    <button class="btn btn-sm" onclick={addColumn}>
      <Icon name="plus" size={11} />
      Columna
    </button>
  </div>

  <!-- Los rótulos van una sola vez arriba y no repetidos en cada fila: con seis columnas, el
       formulario se leía como una lista de etiquetas y no como una tabla. -->
  <div class="mt-2 grid grid-cols-[8rem_10rem_8rem_9rem_auto_1.75rem] gap-1.5 px-2 text-[11px] muted">
    <span>Nombre</span>
    <span>Tipo</span>
    <span>Identidad</span>
    <span>Default</span>
    <span></span>
    <span></span>
  </div>

  <div class="mt-1 flex flex-col gap-1.5">
    {#each columns as column (column.key)}
      <div
        class="grid grid-cols-[8rem_10rem_8rem_9rem_auto_1.75rem] items-center gap-1.5 rounded-md
               border border-zinc-200 p-1.5 dark:border-zinc-800"
      >
        <input class="field" placeholder="nombre" bind:value={column.name} />
        <input
          class="field"
          placeholder="tipo"
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
            <option value={option.value}>{option.label}</option>
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
          title="Quitar la columna"
          aria-label="Quitar la columna"
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
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      Crear tabla
    </button>
  {/snippet}
</Modal>
