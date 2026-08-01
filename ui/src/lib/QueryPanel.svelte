<script lang="ts">
  import HistoryPanel from "./HistoryPanel.svelte";
  import Icon from "./Icon.svelte";
  import PlanTree from "./PlanTree.svelte";
  import ResultGrid from "./ResultGrid.svelte";
  import SqlEditor from "./SqlEditor.svelte";
  import { decimal } from "./format";
  import { queries, type QueryTab } from "./query.svelte";
  import { describeError, explainWarning, statementAtCursor, type ExplainOptions } from "./ipc";

  let { tab }: { tab: QueryTab } = $props();

  let editorHeight = $state(240);
  let pending = $state<{ sql: string; base: number; options: ExplainOptions; warning: string } | null>(
    null,
  );

  const result = $derived(tab.result);
  const withRows = $derived(result?.outcome.kind === "rows" ? result.outcome : null);
  const errors = $derived(tab.messages.filter((message) => message.tone === "error").length);

  /** El nodo más caro del plan, para que la barra de cada uno signifique algo. */
  const worst = $derived.by(() => {
    if (!tab.plan) return 0;
    const walk = (node: typeof tab.plan.root): number =>
      Math.max(node.selfMs ?? 0, ...node.children.map(walk));
    return walk(tab.plan.root);
  });

  /**
   * Qué ejecutar cuando no se pidió el script entero: lo seleccionado si hay selección, y si no la
   * sentencia donde está el cursor. Quién decide dónde termina una sentencia es el núcleo.
   */
  async function resolve(selection: string, cursor: number) {
    if (selection.trim() !== "") return { sql: selection, base: cursor };

    const statement = await statementAtCursor(tab.sql, cursor);
    return statement ? { sql: statement.text, base: statement.offset } : null;
  }

  async function run(selection: string, cursor: number) {
    const target = await resolve(selection, cursor);
    if (target) await queries.run(tab, target.sql, target.base);
  }

  async function explain(selection: string, cursor: number, options: ExplainOptions) {
    const target = await resolve(selection, cursor);
    if (!target) return;

    try {
      // El aviso lo decide el núcleo para que la CLI advierta exactamente lo mismo.
      const warning = await explainWarning(target.sql, options);
      if (warning) {
        pending = { ...target, options, warning };
        return;
      }
    } catch (error) {
      tab.log("error", describeError(error));
    }

    await queries.explain(tab, target.sql, target.base, options);
  }

  function startResize(event: MouseEvent) {
    event.preventDefault();
    const origin = event.clientY;
    const initial = editorHeight;

    const move = (moved: MouseEvent) => {
      editorHeight = Math.min(720, Math.max(96, initial + moved.clientY - origin));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  const analyze: ExplainOptions = { analyze: true, buffers: true, verbose: false };
  const estimate: ExplainOptions = { analyze: false, buffers: false, verbose: false };

  /** El editor entrega la selección y el cursor; el panel decide qué hacer con ellos. */
  let lastCursor = $state(0);
</script>

<div class="flex h-full flex-col">
  <header class="divider-b flex flex-wrap items-center gap-1.5 px-2 py-1.5">
    {#if tab.running}
      <button class="btn" onclick={() => queries.cancel(tab)}>
        <Icon name="close" size={12} />
        Cancelar
      </button>
    {:else}
      <button
        class="btn btn-primary"
        disabled={tab.tabId === null}
        title="Ejecuta la selección, o la sentencia donde está el cursor (Ctrl+Enter)"
        onclick={() => run("", lastCursor)}
      >
        <Icon name="play" size={12} />
        Ejecutar
      </button>
      <button
        class="btn"
        disabled={tab.tabId === null}
        title="Ejecuta todas las sentencias del editor (Ctrl+Shift+Enter)"
        onclick={() => queries.run(tab, tab.sql)}
      >
        Script entero
      </button>
    {/if}

    <span class="mx-1 h-4 w-px bg-zinc-200 dark:bg-zinc-800"></span>

    <button
      class="btn"
      disabled={tab.tabId === null || tab.running}
      title="Muestra el plan estimado sin ejecutar la consulta"
      onclick={() => explain("", lastCursor, estimate)}
    >
      Explicar
    </button>
    <button
      class="btn"
      disabled={tab.tabId === null || tab.running}
      title="Ejecuta la consulta y muestra los tiempos reales"
      onclick={() => explain("", lastCursor, analyze)}
    >
      Explicar y medir
    </button>

    <span class="ml-auto flex items-center gap-2 text-xs muted">
      {#if tab.running}
        <span class="spinner"></span>
      {/if}
      <span title="Base sobre la que corre esta pestaña">{tab.database}</span>
    </span>
  </header>

  <div class="min-h-0 shrink-0 overflow-hidden" style="height: {editorHeight}px">
    <SqlEditor
      bind:value={tab.sql}
      schema={tab.schema}
      errorMark={tab.errorMark}
      onrun={(selection, cursor) => {
        lastCursor = cursor;
        run(selection, cursor);
      }}
      onrunScript={() => queries.run(tab, tab.sql)}
      oncancel={() => queries.cancel(tab)}
    />
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="h-px shrink-0 cursor-row-resize bg-zinc-200 transition-colors hover:bg-blue-400
           dark:bg-zinc-800"
    onmousedown={startResize}
  ></div>

  <div class="flex min-h-0 flex-1 flex-col">
    <div class="divider-b flex items-center gap-1 px-2 py-1">
      <div class="seg" role="tablist">
        {#each [["rows", "Resultados"], ["plan", "Plan"], ["messages", "Mensajes"], ["history", "Historial"]] as [value, label] (value)}
          <button
            class="seg-item"
            role="tab"
            aria-selected={tab.view === value}
            onclick={() => (tab.view = value as typeof tab.view)}
          >
            {label}
            {#if value === "messages" && errors > 0}
              <span class="ml-1 text-rose-600 dark:text-rose-400">•</span>
            {/if}
          </button>
        {/each}
      </div>

      {#if tab.results.length > 1}
        <select
          class="field ml-1 w-40 py-0.5 text-xs"
          title="El script devolvió más de un resultado"
          value={tab.shown}
          onchange={(event) => (tab.shown = Number(event.currentTarget.value))}
        >
          {#each tab.results as item, index (index)}
            <option value={index}>Sentencia {item.index + 1} (línea {item.line})</option>
          {/each}
        </select>
      {/if}

      {#if withRows}
        <span class="ml-auto text-xs muted">
          {withRows.rowCount}
          {withRows.rowCount === 1 ? "fila" : "filas"}
          {#if withRows.truncated}· se muestran {withRows.rows.length}{/if}
          · {decimal(withRows.seconds * 1000, 0)} ms
        </span>
      {/if}
    </div>

    <div class="min-h-0 flex-1">
      {#if tab.view === "rows"}
        {#if withRows}
          {#key result}
            <ResultGrid columns={withRows.columns} rows={withRows.rows} />
          {/key}
        {:else if result}
          <p class="p-4 text-sm muted">
            {result.outcome.kind === "command"
              ? `${result.outcome.tag}: ${result.outcome.affected}`
              : ""}
          </p>
        {:else}
          <p class="p-4 text-sm muted">
            Escribí una consulta y ejecutala con Ctrl+Enter.
          </p>
        {/if}
      {:else if tab.view === "plan"}
        {#if tab.plan}
          <div class="h-full overflow-auto p-2">
            <PlanTree node={tab.plan.root} {worst} />
            <p class="mt-3 px-2 text-xs muted">
              {#if tab.plan.planningMs !== null}
                planificación {decimal(tab.plan.planningMs, 2)} ms
              {/if}
              {#if tab.plan.executionMs !== null}
                · ejecución {decimal(tab.plan.executionMs, 2)} ms
              {/if}
              {#if !tab.plan.analyzed}
                · plan estimado, sin ejecutar
              {/if}
            </p>
          </div>
        {:else}
          <p class="p-4 text-sm muted">Pedí «Explicar» para ver cómo resolvería la consulta.</p>
        {/if}
      {:else if tab.view === "history"}
        <HistoryPanel
          profileId={tab.profileId}
          onpick={(sql) => {
            tab.sql = sql;
            tab.view = "rows";
          }}
        />
      {:else if tab.messages.length === 0}
        <p class="p-4 text-sm muted">Sin mensajes.</p>
      {:else}
        <ul class="h-full overflow-auto p-2 font-mono text-xs">
          {#each tab.messages as message, index (index)}
            <li
              class="px-2 py-0.5 {message.tone === 'error'
                ? 'text-rose-600 dark:text-rose-400'
                : message.tone === 'notice'
                  ? 'text-amber-700 dark:text-amber-400'
                  : 'muted'}"
            >
              {message.text}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
</div>

{#if pending}
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
    <div class="card w-full max-w-md p-5 shadow-xl">
      <h2 class="text-base font-medium">Esto ejecuta la sentencia</h2>
      <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">{pending.warning}</p>
      <div class="mt-4 flex justify-end gap-2">
        <button class="btn" onclick={() => (pending = null)}>Cancelar</button>
        <button
          class="btn btn-primary"
          onclick={() => {
            const target = pending;
            pending = null;
            if (target) queries.explain(tab, target.sql, target.base, target.options);
          }}
        >
          Ejecutar igual
        </button>
      </div>
    </div>
  </div>
{/if}
