<script lang="ts" module>
  /** Un resultado con filas que se puede poner de un lado de la comparación. */
  export interface CompareSource {
    id: string;
    label: string;
    columns: string[];
    rows: (string | null)[][];
    /** El resultado vino cortado por el techo: lo que se compara no es todo. */
    truncated: boolean;
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { commonColumns, diffResults, diffSummary } from "./result-diff";

  let { sources, initialLeft, onclose }: {
    sources: CompareSource[];
    /** El que se estaba mirando; el derecho arranca en otro. */
    initialLeft: string;
    onclose: () => void;
  } = $props();

  /** Más filas que esto en pantalla no se leen; el resumen igual cuenta todas. */
  const SHOWN_MAX = 500;
  const NULL = "NULL";

  let leftId = $state(untrack(() => initialLeft));
  let rightId = $state(
    untrack(() => sources.find((source) => source.id !== initialLeft)?.id ?? initialLeft),
  );
  /** `""` = emparejar por posición. */
  let keyColumn = $state("");
  let onlyDiffs = $state(true);

  const left = $derived(sources.find((source) => source.id === leftId) ?? sources[0]);
  const right = $derived(sources.find((source) => source.id === rightId) ?? sources[0]);
  const keys = $derived(commonColumns(left, right));
  // Cambiar de resultado puede dejar la clave elegida sin existir de un lado.
  const effectiveKey = $derived(keyColumn !== "" && keys.includes(keyColumn) ? keyColumn : "");

  const diff = $derived(diffResults(left, right, { key: effectiveKey === "" ? null : [effectiveKey] }));
  const visible = $derived(
    (onlyDiffs ? diff.rows.filter((row) => row.status !== "same") : diff.rows).slice(0, SHOWN_MAX),
  );
  const hidden = $derived(
    (onlyDiffs ? diff.rows.filter((row) => row.status !== "same").length : diff.rows.length) -
      visible.length,
  );

  function show(value: string | null | undefined): string {
    return value === null || value === undefined ? NULL : value;
  }
</script>

<Modal title="Comparar resultados" subtitle={diffSummary(diff)} size="xl" {onclose}>
  <div class="grid grid-cols-2 gap-3">
    <label class="flex min-w-0 flex-col gap-1">
      <span class="label">Resultado A (antes)</span>
      <select class="field" bind:value={leftId}>
        {#each sources as source (source.id)}
          <option value={source.id}>{source.label}</option>
        {/each}
      </select>
    </label>
    <label class="flex min-w-0 flex-col gap-1">
      <span class="label">Resultado B (después)</span>
      <select class="field" bind:value={rightId}>
        {#each sources as source (source.id)}
          <option value={source.id}>{source.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="mt-3 flex flex-wrap items-center gap-3">
    <label class="flex items-center gap-1.5 text-xs muted">
      <span>Emparejar por</span>
      <select class="field py-0.5 text-xs" bind:value={keyColumn}>
        <option value="">posición de la fila</option>
        {#each keys as name (name)}
          <option value={name}>columna {name}</option>
        {/each}
      </select>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={onlyDiffs} />
      Solo las diferencias
    </label>
  </div>

  {#if left.truncated || right.truncated}
    <Alert tone="warn" box class="mt-3">
      Uno de los resultados vino cortado por el techo de filas: se compara lo que se trajo, no la
      consulta entera.
    </Alert>
  {/if}
  {#if diff.onlyLeft.length > 0 || diff.onlyRight.length > 0}
    <Alert tone="warn" box class="mt-3">
      Se comparan las columnas con el mismo nombre en los dos lados.
      {#if diff.onlyLeft.length > 0}Solo en A: {diff.onlyLeft.join(", ")}.{/if}
      {#if diff.onlyRight.length > 0}Solo en B: {diff.onlyRight.join(", ")}.{/if}
    </Alert>
  {/if}

  {#if diff.columns.length === 0}
    <Alert tone="bad" box class="mt-3">Los dos resultados no tienen ninguna columna en común.</Alert>
  {:else}
    <div class="table-scroll mt-3 max-h-[50vh] overflow-y-auto">
      <table class="list-table font-mono text-xs">
        <thead>
          <tr>
            <th>{effectiveKey === "" ? "#" : effectiveKey}</th>
            {#each diff.columns as name (name)}<th>{name}</th>{/each}
          </tr>
        </thead>
        <tbody>
          {#each visible as row, index (index)}
            <tr
              class={row.status === "added"
                ? "diff-row-added"
                : row.status === "removed"
                  ? "diff-row-removed"
                  : ""}
            >
              <td class="muted">
                {row.key ?? index + 1}
                {#if row.status === "added"}<span title="Solo en B"> +</span>{/if}
                {#if row.status === "removed"}<span title="Solo en A"> −</span>{/if}
              </td>
              {#each diff.columns as _, c (c)}
                <td class={row.changed[c] ? "diff-cell-changed" : ""}>
                  {#if row.changed[c]}
                    <span class="diff-old">{show(row.left?.[c])}</span>
                    <span class="diff-new">{show(row.right?.[c])}</span>
                  {:else}
                    {show((row.left ?? row.right)?.[c])}
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
          {#if visible.length === 0}
            <tr><td colspan={diff.columns.length + 1} class="muted">Sin diferencias.</td></tr>
          {/if}
        </tbody>
      </table>
    </div>
    {#if hidden > 0}
      <p class="mt-1 text-xs muted">Se muestran {SHOWN_MAX}; faltan {hidden} filas más.</p>
    {/if}
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
  {/snippet}
</Modal>
