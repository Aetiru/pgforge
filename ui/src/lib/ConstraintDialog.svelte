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
    type ConstraintDef,
    type RefAction,
    type TableChange,
    type TableColumn,
  } from "./ipc";

  let {
    profileId,
    database,
    schema,
    table,
    columns,
    onclose,
    oncreated,
  }: {
    profileId: string;
    database: string;
    schema: string;
    table: string;
    columns: TableColumn[];
    onclose: () => void;
    oncreated: () => void;
  } = $props();

  type Kind = "primaryKey" | "unique" | "foreignKey" | "check";

  const KIND_OPTIONS: { value: Kind; label: string }[] = [
    { value: "primaryKey", label: "Primary Key" },
    { value: "unique", label: "Unique" },
    { value: "foreignKey", label: "Foreign Key" },
    { value: "check", label: "Check" },
  ];

  const ACTION_OPTIONS: { value: RefAction | ""; label: string }[] = [
    { value: "", label: "Sin acción" },
    { value: "cascade", label: "CASCADE" },
    { value: "setNull", label: "SET NULL" },
    { value: "setDefault", label: "SET DEFAULT" },
    { value: "restrict", label: "RESTRICT" },
    { value: "noAction", label: "NO ACTION" },
  ];

  let name = $state("");
  let kind = $state<Kind>("primaryKey");
  let selected = $state<string[]>([]);
  // Copia editable, tomada una sola vez: el esquema referenciado por omisión es el de la tabla,
  // pero el campo se puede cambiar sin que un re-render lo pise.
  let refSchema = $state(untrack(() => schema));
  let refTable = $state("");
  let refColumns = $state("");
  let onDelete = $state<RefAction | "">("");
  let onUpdate = $state<RefAction | "">("");
  let expression = $state("");

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function toggle(column: string) {
    selected = selected.includes(column)
      ? selected.filter((name) => name !== column)
      : [...selected, column];
  }

  function definition(): ConstraintDef {
    switch (kind) {
      case "primaryKey":
        return { kind: "primaryKey", columns: selected };
      case "unique":
        return { kind: "unique", columns: selected };
      case "foreignKey":
        return {
          kind: "foreignKey",
          columns: selected,
          refSchema: refSchema.trim(),
          refTable: refTable.trim(),
          refColumns: refColumns
            .split(",")
            .map((c) => c.trim())
            .filter((c) => c.length > 0),
          onDelete: onDelete || null,
          onUpdate: onUpdate || null,
        };
      case "check":
        return { kind: "check", expression: expression.trim() };
    }
  }

  function change(): TableChange {
    return { kind: "addConstraint", schema, table, name: name.trim(), definition: definition() };
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para la constraint.";
    if (kind === "primaryKey" || kind === "unique") {
      if (selected.length === 0) return "Elegí al menos una columna.";
    } else if (kind === "foreignKey") {
      if (selected.length === 0) return "Elegí al menos una columna local.";
      if (!refTable.trim()) return "Poné la tabla referenciada.";
      const def = definition() as Extract<ConstraintDef, { kind: "foreignKey" }>;
      if (def.refColumns.length === 0) return "Poné las columnas referenciadas.";
    } else if (kind === "check") {
      if (!expression.trim()) return "Poné la expresión del CHECK.";
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

    if (!(await confirmMutation(profileId, "Se va a crear una restricción."))) return;

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

<Modal title="Nueva restricción" subtitle="{schema}.{table}" busy={saving} {onclose}>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre</span>
      <input class="field" data-autofocus bind:value={name} />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Tipo</span>
      <select class="field" bind:value={kind}>
        {#each KIND_OPTIONS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if kind === "primaryKey" || kind === "unique" || kind === "foreignKey"}
    <div class="mt-3">
      <span class="label">{kind === "foreignKey" ? "Columnas locales" : "Columnas"}</span>
      <div
        class="mt-1 flex max-h-52 flex-col gap-1 overflow-auto rounded-md border border-zinc-200 p-2
               dark:border-zinc-700"
      >
        {#each columns as column (column.name)}
          <label class="check text-xs">
            <input
              type="checkbox"
              checked={selected.includes(column.name)}
              onchange={() => toggle(column.name)}
            />
            {column.name}
            <span class="muted">{column.typeName}</span>
          </label>
        {/each}
      </div>
    </div>
  {/if}

  {#if kind === "foreignKey"}
    <div class="mt-3 grid grid-cols-2 gap-3">
      <label class="flex flex-col gap-1">
        <span class="label">Esquema referenciado</span>
        <input class="field" bind:value={refSchema} />
      </label>
      <label class="flex flex-col gap-1">
        <span class="label">Tabla referenciada</span>
        <input class="field" bind:value={refTable} />
      </label>
      <label class="col-span-2 flex flex-col gap-1">
        <span class="label">Columnas referenciadas (separadas por coma)</span>
        <input class="field" bind:value={refColumns} placeholder="id" />
      </label>
      <label class="flex flex-col gap-1">
        <span class="label">ON DELETE</span>
        <select class="field" bind:value={onDelete}>
          {#each ACTION_OPTIONS as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
      <label class="flex flex-col gap-1">
        <span class="label">ON UPDATE</span>
        <select class="field" bind:value={onUpdate}>
          {#each ACTION_OPTIONS as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
    </div>
  {/if}

  {#if kind === "check"}
    <label class="mt-3 flex flex-col gap-1">
      <span class="label">Expresión</span>
      <textarea class="field font-mono" rows="3" bind:value={expression} placeholder="total >= 0"
      ></textarea>
    </label>
  {/if}

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
      Agregar
    </button>
  {/snippet}
</Modal>
