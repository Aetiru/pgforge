<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    ddlApply,
    ddlPreview,
    describeError,
    type Identity,
    type TableChange,
    type TableColumn,
  } from "./ipc";

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

  // Copia editable, tomada una sola vez: en modo edición es lo que se compara contra el original
  // para mandar solo lo que de verdad cambió.
  let name = $state(untrack(() => column?.name ?? ""));
  let typeName = $state(untrack(() => column?.typeName ?? ""));
  let notNull = $state(untrack(() => column?.notNull ?? false));
  let defaultValue = $state(untrack(() => column?.default ?? ""));
  let identity = $state<Identity | "">("");
  let using = $state("");

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  const typeChanged = $derived(column !== null && typeName.trim() !== column.typeName);

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para la columna.";
    if (!typeName.trim()) return "Poné un tipo para la columna.";
    return null;
  }

  /** Los cambios pendientes. En alta siempre hay uno; en edición, solo lo que se tocó. */
  function changes(): TableChange[] {
    if (!column) {
      return [
        {
          kind: "addColumn",
          schema,
          table,
          column: {
            name: name.trim(),
            typeName: typeName.trim(),
            notNull,
            default: identity ? null : defaultValue.trim() || null,
            identity: identity || null,
          },
        },
      ];
    }

    const out: TableChange[] = [];
    // El renombre va primero: los pasos siguientes ya tienen que referirse al nombre nuevo, porque
    // se ejecutan en orden dentro de la misma transacción.
    let current = column.name;
    if (name.trim() !== column.name) {
      out.push({ kind: "renameColumn", schema, table, column: current, newName: name.trim() });
      current = name.trim();
    }
    if (typeName.trim() !== column.typeName) {
      out.push({
        kind: "alterColumnType",
        schema,
        table,
        column: current,
        typeName: typeName.trim(),
        using: using.trim() || null,
      });
    }
    if (notNull !== column.notNull) {
      out.push({ kind: "setColumnNotNull", schema, table, column: current, notNull });
    }
    const original = column.default ?? "";
    if (defaultValue.trim() !== original.trim()) {
      out.push({
        kind: "setColumnDefault",
        schema,
        table,
        column: current,
        default: defaultValue.trim() || null,
      });
    }
    return out;
  }

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
      <input class="field" data-autofocus bind:value={name} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Tipo</span>
      <input class="field" list="pgforge-column-types" bind:value={typeName} />
    </label>

    {#if column && typeChanged}
      <label class="col-span-2 flex flex-col gap-1">
        <span class="label">USING (opcional, solo si el cambio de tipo no es implícito)</span>
        <input class="field" placeholder={`${column.name}::${typeName}`} bind:value={using} />
      </label>
    {/if}

    {#if !column}
      <label class="flex flex-col gap-1">
        <span class="label">Identidad</span>
        <select class="field" bind:value={identity}>
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
        disabled={!column && identity !== ""}
        bind:value={defaultValue}
        placeholder="expresión SQL, p. ej. now()"
      />
    </label>

    <label class="check col-span-2">
      <input type="checkbox" bind:checked={notNull} />
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
