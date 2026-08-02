<script lang="ts">
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

<div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
  <div
    class="card flex max-h-[85vh] w-full max-w-lg flex-col shadow-xl"
    role="dialog"
    aria-modal="true"
    aria-label="Nuevo índice"
  >
    <h2 class="divider-b px-5 py-3 text-base font-medium">Nuevo índice en {table}</h2>

    <div class="min-h-0 flex-1 overflow-auto px-5 py-4 text-sm">
      <div class="grid grid-cols-2 gap-3">
        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Nombre (opcional)</span>
          <input class="field" bind:value={name} placeholder="lo nombra Postgres" />
        </label>

        <label class="flex flex-col gap-1">
          <span class="text-xs muted">Método</span>
          <select class="field" bind:value={method}>
            {#each METHODS as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </label>
      </div>

      <div class="mt-3">
        <span class="text-xs muted">Columnas</span>
        <div class="mt-1 flex flex-col gap-1 rounded border border-zinc-200 p-2 dark:border-zinc-800">
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

      <label class="mt-3 flex flex-col gap-1">
        <span class="text-xs muted">WHERE (opcional, índice parcial)</span>
        <input class="field" bind:value={whereClause} placeholder="expresión SQL" />
      </label>

      <div class="mt-3 flex gap-4">
        <label class="check text-xs">
          <input type="checkbox" bind:checked={unique} />
          Único
        </label>
        <label class="check text-xs">
          <input type="checkbox" bind:checked={concurrently} />
          CONCURRENTLY (no bloquea la tabla mientras se construye)
        </label>
      </div>

      {#if error}
        <p class="mt-3 text-sm text-rose-600 dark:text-rose-400">{error}</p>
      {/if}

      {#if preview}
        <pre
          class="mt-3 max-h-32 overflow-auto rounded bg-zinc-100 p-2 font-mono text-xs
                 whitespace-pre-wrap select-text dark:bg-zinc-800">{preview}</pre>
      {/if}
    </div>

    <div class="divider-t flex items-center gap-2 px-5 py-3">
      <button class="btn btn-ghost text-xs" onclick={showPreview} disabled={saving}>
        Ver SQL
      </button>
      <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
      <button class="btn btn-primary" onclick={submit} disabled={saving}>Crear índice</button>
    </div>
  </div>
</div>
