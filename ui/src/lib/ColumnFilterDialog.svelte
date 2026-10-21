<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { FILTER_OPS, clause, type FilterOp } from "./column-filter";

  let { column, columns, types, preset, onapply, onclose }: {
    column: string;
    /** Nombres y tipos reales de la tabla: contra ellos se valida el nombre de la columna. */
    columns: string[];
    types: string[];
    preset?: { op: FilterOp; value?: string };
    /** Recibe la condición ya armada, lista para sumarla al `WHERE`. */
    onapply: (sql: string) => void;
    onclose: () => void;
  } = $props();

  let op = $state<FilterOp>(untrack(() => preset?.op ?? "eq"));
  let value = $state(untrack(() => preset?.value ?? ""));
  let error = $state<string | null>(null);

  const needsValue = $derived(FILTER_OPS.find((item) => item.value === op)?.needsValue ?? false);
  // Lo que se va a agregar, a la vista antes de aplicar.
  const built = $derived(clause({ column, op, value: needsValue ? value : undefined }, columns, types));

  function submit() {
    if ("error" in built) {
      error = built.error;
      return;
    }
    onapply(built.sql);
    onclose();
  }
</script>

<Modal title="Filtrar «{column}»" subtitle="Se suma al WHERE con AND" size="sm" {onclose}>
  <div class="space-y-3">
    <label class="flex flex-col gap-1">
      <span class="label">Condición</span>
      <select class="field" bind:value={op}>
        {#each FILTER_OPS as item (item.value)}
          <option value={item.value}>{item.label}</option>
        {/each}
      </select>
    </label>

    {#if needsValue}
      <label class="flex flex-col gap-1">
        <span class="label">Valor</span>
        <input
          class="field font-mono"
          data-autofocus
          bind:value
          spellcheck="false"
          onkeydown={(event) => {
            if (event.key === "Enter") submit();
          }}
        />
      </label>
    {/if}

    <p class="text-xs muted">
      El valor se escribe como texto entre comillas: no hace falta escaparlo. «Contiene» no
      distingue mayúsculas.
    </p>
    {#if "sql" in built}
      <pre class="rounded bg-zinc-50 p-2 font-mono text-xs break-words whitespace-pre-wrap select-text dark:bg-zinc-800/60">{built.sql}</pre>
    {/if}
    {#if error}<Alert tone="bad" box>{error}</Alert>{/if}
  </div>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit}>Aplicar</button>
  {/snippet}
</Modal>
