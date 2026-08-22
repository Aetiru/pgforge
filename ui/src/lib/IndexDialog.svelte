<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    INDEX_METHODS,
    indexDef,
    indexForm,
    toggleColumn,
    validateIndex,
  } from "./index-form";
  import { tasks } from "./tasks.svelte";
  import { describeError, indexPreview, type TableColumn } from "./ipc";

  let {
    profileId,
    database,
    schema,
    table,
    columns,
    initialColumns = [],
    onclose,
    oncreated,
  }: {
    profileId: string;
    database: string;
    schema: string;
    table: string;
    columns: TableColumn[];
    /** Columnas ya elegidas al abrir: es lo que trae la sugerencia de un plan de ejecución. */
    initialColumns?: string[];
    onclose: () => void;
    oncreated: () => void;
  } = $props();

  // Se toma una sola vez, como el resto de los formularios: a partir de acá el dueño es el usuario.
  let form = $state(untrack(() => indexForm(initialColumns)));

  let error = $state<string | null>(null);
  let preview = $state<string | null>(null);

  const toggle = (column: string) => (form.columns = toggleColumn(form.columns, column));
  const def = () => indexDef(form, { schema, table });
  const validate = () => validateIndex(form);

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

    if (!(await confirmMutation(profileId, "Se va a crear un índice."))) return;

    // Se larga y se cierra: `CONCURRENTLY` sobre una tabla grande tarda lo suyo, y esperarlo con el
    // diálogo abierto dejaba la aplicación tomada. La lista de índices se relee cuando el índice
    // existe de verdad, no ahora.
    try {
      await tasks.index({ profileId, database, def: def(), onDone: oncreated });
    } catch (e) {
      error = describeError(e);
      return;
    }
    onclose();
  }
</script>

<Modal title="Nuevo índice" subtitle="{schema}.{table}" {onclose}>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Nombre (opcional)</span>
      <input class="field" data-autofocus bind:value={form.name} placeholder="lo nombra Postgres" />
    </label>

    <label class="flex flex-col gap-1">
      <span class="label">Método</span>
      <select class="field" bind:value={form.method}>
        {#each INDEX_METHODS as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="mt-3">
    <span class="label">Columnas {form.columns.length > 0 ? `(${form.columns.join(", ")})` : ""}</span>
    <div
      class="mt-1 flex max-h-52 flex-col gap-1 overflow-auto rounded-md border border-zinc-200 p-2
             dark:border-zinc-700"
    >
      {#each columns as column (column.name)}
        <label class="check text-xs">
          <input
            type="checkbox"
            checked={form.columns.includes(column.name)}
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
    <input class="field" bind:value={form.whereClause} placeholder="expresión SQL" />
  </label>

  <div class="mt-3 flex flex-wrap gap-4">
    <label class="check">
      <input type="checkbox" bind:checked={form.unique} />
      Único
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={form.concurrently} />
      CONCURRENTLY (no bloquea la tabla mientras se construye)
    </label>
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  <p class="mt-3 text-xs muted">
    Se crea en segundo plano: se sigue y se cancela desde la vista de procesos.
  </p>

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview}>Ver SQL</button>
    <button class="btn ml-auto" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit}>Crear índice</button>
  {/snippet}
</Modal>
