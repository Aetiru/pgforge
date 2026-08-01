<script lang="ts">
  import Icon from "./Icon.svelte";
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

<div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
  <div
    class="card flex max-h-[85vh] w-full max-w-2xl flex-col shadow-xl"
    role="dialog"
    aria-modal="true"
    aria-label="Nueva tabla"
  >
    <h2 class="divider-b px-5 py-3 text-base font-medium">Nueva tabla en {schema}</h2>

    <div class="min-h-0 flex-1 overflow-auto px-5 py-4 text-sm">
      <label class="flex flex-col gap-1">
        <span class="text-xs muted">Nombre</span>
        <input class="field" bind:value={name} placeholder="clientes" />
      </label>

      <div class="mt-4 flex items-center justify-between">
        <span class="text-xs font-medium muted">Columnas</span>
        <button class="btn btn-ghost px-2 py-0.5 text-xs" onclick={addColumn}>
          <Icon name="plus" size={12} />
          Columna
        </button>
      </div>

      <div class="mt-2 flex flex-col gap-2">
        {#each columns as column (column.key)}
          <div class="flex flex-wrap items-center gap-1.5 rounded border border-zinc-200 p-2 dark:border-zinc-800">
            <input class="field w-32" placeholder="nombre" bind:value={column.name} />
            <input
              class="field w-40"
              placeholder="tipo"
              list="pgforge-common-types"
              bind:value={column.typeName}
            />
            <select
              class="field w-32"
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
              class="field w-36"
              placeholder="default"
              disabled={column.identity !== null}
              value={column.default ?? ""}
              oninput={(event) => (column.default = event.currentTarget.value || null)}
            />
            <label class="check text-xs">
              <input type="checkbox" bind:checked={column.notNull} />
              NOT NULL
            </label>
            <button
              class="btn btn-ghost ml-auto px-2 py-0.5 text-xs"
              disabled={columns.length === 1}
              onclick={() => removeColumn(column.key)}
            >
              <Icon name="close" size={10} />
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
        <p class="mt-3 text-sm text-rose-600 dark:text-rose-400">{error}</p>
      {/if}

      {#if preview}
        <pre
          class="mt-3 max-h-40 overflow-auto rounded bg-zinc-100 p-2 font-mono text-xs
                 whitespace-pre-wrap select-text dark:bg-zinc-800">{preview}</pre>
      {/if}
    </div>

    <div class="divider-t flex items-center gap-2 px-5 py-3">
      <button class="btn btn-ghost text-xs" onclick={showPreview} disabled={saving}>
        Ver SQL
      </button>
      <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
      <button class="btn btn-primary" onclick={submit} disabled={saving}>Crear tabla</button>
    </div>
  </div>
</div>
