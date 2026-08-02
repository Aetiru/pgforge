<script lang="ts">
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import { describeError, indexCreate, indexPreview, type IndexDef, type TableColumn } from "./ipc";

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

  const METHODS = [
    { value: "", label: "btree (por omisión)" },
    { value: "gin", label: "gin" },
    { value: "gist", label: "gist" },
    { value: "hash", label: "hash" },
    { value: "brin", label: "brin" },
    { value: "spgist", label: "spgist" },
  ];

  let name = $state("");
  let unique = $state(false);
  let method = $state("");
  let selected = $state<string[]>([]);
  let whereClause = $state("");
  let concurrently = $state(false);

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function toggle(column: string) {
    selected = selected.includes(column)
      ? selected.filter((name) => name !== column)
      : [...selected, column];
  }

  function def(): IndexDef {
    return {
      schema,
      table,
      name: name.trim() || null,
      unique,
      method: method || null,
      columns: selected,
      whereClause: whereClause.trim() || null,
      concurrently,
    };
  }

  function validate(): string | null {
    if (selected.length === 0) return "Elegí al menos una columna.";
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
      preview = (await indexPreview(def())).sql;
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
      await indexCreate(profileId, def(), database);
      oncreated();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title="Nuevo índice" subtitle="{schema}.{table}" busy={saving} {onclose}>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre (opcional)</span>
      <input class="field" data-autofocus bind:value={name} placeholder="lo nombra Postgres" />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Método</span>
      <select class="field" bind:value={method}>
        {#each METHODS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="mt-3">
    <span class="label">Columnas {selected.length > 0 ? `(${selected.join(", ")})` : ""}</span>
    <div
      class="mt-1 flex max-h-52 flex-col gap-1 overflow-auto rounded-md border border-zinc-200 p-2
             dark:border-zinc-800"
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
    <p class="mt-1 text-[11px] muted">El orden en que se marcan es el orden del índice.</p>
  </div>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">WHERE (opcional, índice parcial)</span>
    <input class="field" bind:value={whereClause} placeholder="expresión SQL" />
  </label>

  <div class="mt-3 flex flex-wrap gap-4">
    <label class="check">
      <input type="checkbox" bind:checked={unique} />
      Único
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={concurrently} />
      CONCURRENTLY (no bloquea la tabla mientras se construye)
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
      Crear índice
    </button>
  {/snippet}
</Modal>
