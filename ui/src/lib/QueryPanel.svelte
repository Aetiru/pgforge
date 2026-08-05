<script lang="ts">
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import { environmentOf, isReadOnly } from "./access.svelte";
  import { envLook, READ_ONLY_LOOK } from "./badges";
  import HistoryPanel from "./HistoryPanel.svelte";
  import Icon from "./Icon.svelte";
  import PlanTree from "./PlanTree.svelte";
  import ResultGrid from "./ResultGrid.svelte";
  import SqlEditor from "./SqlEditor.svelte";
  import { count, decimal } from "./format";
  import type { QueryTab } from "./query.svelte";
  import { describeError, explainWarning, statementAtCursor, type ExplainOptions } from "./ipc";

  let { tab }: { tab: QueryTab } = $props();

  let editorHeight = $state(240);
  let pending = $state<{
    sql: string;
    base: number;
    options: ExplainOptions;
    warning: string;
  } | null>(null);

  const environment = $derived(environmentOf(tab.profileId));
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
    if (target) await tab.run(target.sql, target.base);
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

    await tab.explain(target.sql, target.base, options);
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
      document.body.classList.remove("cursor-row-resize");
    };
    document.body.classList.add("cursor-row-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  const analyze: ExplainOptions = { analyze: true, buffers: true, verbose: false };
  const estimate: ExplainOptions = { analyze: false, buffers: false, verbose: false };

  /** El editor entrega la selección y el cursor; el panel decide qué hacer con ellos. */
  let lastCursor = $state(0);

  const VIEWS = [
    { value: "rows", label: "Resultados" },
    { value: "plan", label: "Plan" },
    { value: "messages", label: "Mensajes" },
    { value: "history", label: "Historial" },
  ] as const;
</script>

<div class="flex h-full flex-col">
  <header class="toolbar">
    {#if tab.running}
      <button class="btn btn-danger" onclick={() => tab.cancel()}>
        <Icon name="close" size={12} />
        Cancelar
      </button>
    {:else}
      <button
        class="btn btn-primary"
        disabled={tab.tabId === null}
        title="Ejecuta la selección, o la sentencia donde está el cursor"
        onclick={() => run("", lastCursor)}
      >
        <Icon name="play" size={12} />
        Ejecutar
        <span class="kbd border-white/30 bg-white/15 text-white/80">Ctrl+↵</span>
      </button>
      <button
        class="btn"
        disabled={tab.tabId === null}
        title="Ejecuta todas las sentencias del editor"
        onclick={() => tab.run(tab.sql)}
      >
        Script entero
        <span class="kbd">Ctrl+⇧+↵</span>
      </button>
    {/if}

    <span class="toolbar-sep"></span>

    <!--
      El interruptor y los dos botones van juntos: apagar el autocommit sin tener a la vista con qué
      confirmar deja al usuario con una transacción abierta y sin dónde cerrarla.
    -->
    <label class="check" title="Apagado, cada ejecución abre una transacción que hay que confirmar">
      <input
        type="checkbox"
        checked={tab.autocommit}
        disabled={tab.tabId === null || tab.running}
        onchange={(event) => tab.setAutocommit(event.currentTarget.checked)}
      />
      Autocommit
    </label>

    <button
      class="btn"
      disabled={tab.tabId === null || tab.running || tab.txStatus === "idle"}
      title="Confirma la transacción abierta en esta pestaña"
      onclick={() => tab.commit()}
    >
      Commit
    </button>
    <button
      class="btn btn-danger"
      disabled={tab.tabId === null || tab.running || tab.txStatus === "idle"}
      title="Descarta todo lo hecho desde que se abrió la transacción"
      onclick={() => tab.rollback()}
    >
      Rollback
    </button>

    <span class="toolbar-sep"></span>

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
        <span class="flex items-center gap-1.5 text-blue-600 dark:text-blue-400">
          <span class="spinner"></span>
          ejecutando…
        </span>
      {/if}
      {#if tab.txStatus !== "idle"}
        <span
          class="tag {tab.txStatus === 'failed' ? 'tag-bad' : 'tag-warn'}"
          title={tab.txStatus === "failed"
            ? "Una sentencia falló dentro de la transacción: el servidor rechaza el resto hasta el rollback"
            : "Hay cambios sin confirmar en esta pestaña"}
        >
          {tab.txStatus === "failed" ? "transacción abortada" : "transacción abierta"}
        </span>
      {/if}
      <!-- Contra qué servidor corre lo que se está por ejecutar es justo lo que no se puede
           adivinar mirando el editor. -->
      {#if environment}
        {@const badge = envLook(environment)}
        <span class="tag {badge.tone}" title={badge.title}>{badge.label}</span>
      {/if}
      {#if isReadOnly(tab.profileId)}
        <span class="tag tag-neutral" title={READ_ONLY_LOOK.title}>{READ_ONLY_LOOK.label}</span>
      {/if}
      <span class="tag tag-neutral" title="Base sobre la que corre esta pestaña">
        <Icon name="database" size={10} />
        {tab.database}
      </span>
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
      onrunScript={() => tab.run(tab.sql)}
      oncancel={() => tab.cancel()}
    />
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="group relative h-px shrink-0 bg-zinc-200 dark:bg-zinc-800"
    onmousedown={startResize}
    ondblclick={() => (editorHeight = 240)}
    title="Arrastrá para cambiar la altura del editor; doble clic para restablecerla"
  >
    <div
      class="absolute inset-x-0 -top-[3px] h-[7px] cursor-row-resize transition-colors
             group-hover:bg-blue-500/40"
    ></div>
  </div>

  <div class="flex min-h-0 flex-1 flex-col">
    <div class="divider-b flex items-center gap-2 px-2 py-1">
      <div class="seg" role="tablist">
        {#each VIEWS as item (item.value)}
          <button
            class="seg-item"
            role="tab"
            aria-selected={tab.view === item.value}
            onclick={() => (tab.view = item.value)}
          >
            {item.label}
            {#if item.value === "messages" && errors > 0}
              <span class="tag tag-bad px-1 py-0 text-[10px]">{errors}</span>
            {/if}
          </button>
        {/each}
      </div>

      {#if tab.results.length > 1}
        <select
          class="field ml-1 w-44 py-0.5 text-xs"
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
        <span class="ml-auto flex items-center gap-2 text-xs muted">
          {#if withRows.truncated}
            <span class="tag tag-warn" title="Hay más filas de las que se trajeron">
              se muestran {count(withRows.rows.length)}
            </span>
          {/if}
          <span class="tabular-nums">
            {count(withRows.rowCount)}
            {withRows.rowCount === 1 ? "fila" : "filas"}
          </span>
          <span class="tabular-nums">{decimal(withRows.seconds * 1000, 0)} ms</span>
        </span>
      {/if}
    </div>

    <div class="min-h-0 flex-1">
      {#if tab.view === "rows"}
        {#if withRows}
          {#key result}
            <ResultGrid columns={withRows.columns} rows={withRows.rows} />
          {/key}
        {:else if result && result.outcome.kind === "command"}
          <Empty
            icon="check"
            title="{result.outcome.tag} · {count(result.outcome.affected)} {result.outcome
              .affected === 1
              ? 'fila'
              : 'filas'}"
            hint="La sentencia no devuelve filas; el servidor informó cuántas tocó."
          />
        {:else}
          <Empty
            icon="sql"
            title="Todavía no ejecutaste nada"
            hint="Escribí una consulta y ejecutala con Ctrl+Enter. Si hay varias sentencias, se ejecuta la del cursor; si hay una selección, se ejecuta la selección."
          />
        {/if}
      {:else if tab.view === "plan"}
        {#if tab.plan}
          <div class="h-full overflow-auto p-2">
            <PlanTree node={tab.plan.root} {worst} />
            <p class="mt-3 flex flex-wrap gap-x-3 px-2 text-xs muted">
              {#if tab.plan.planningMs !== null}
                <span>planificación {decimal(tab.plan.planningMs, 2)} ms</span>
              {/if}
              {#if tab.plan.executionMs !== null}
                <span>ejecución {decimal(tab.plan.executionMs, 2)} ms</span>
              {/if}
              {#if !tab.plan.analyzed}
                <span class="tag tag-neutral">plan estimado, sin ejecutar</span>
              {/if}
            </p>
          </div>
        {:else}
          <Empty
            icon="compass"
            title="Sin plan"
            hint="«Explicar» muestra cómo resolvería PostgreSQL la consulta sin ejecutarla; «Explicar y medir» la ejecuta y compara lo estimado con lo real."
          />
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
        <Empty
          icon="info"
          title="Sin mensajes"
          hint="Acá aparecen los avisos del servidor (NOTICE, WARNING) y los errores de cada sentencia."
        />
      {:else}
        <ul class="h-full overflow-auto p-2 font-mono text-xs select-text">
          {#each tab.messages as message, index (index)}
            <li
              class="flex items-start gap-2 rounded px-2 py-1 {message.tone === 'error'
                ? 'text-rose-600 dark:text-rose-400'
                : message.tone === 'notice'
                  ? 'text-amber-700 dark:text-amber-400'
                  : 'muted'}"
            >
              {#if message.tone !== "info"}
                <Icon name="warn" size={12} class="mt-0.5 shrink-0" />
              {/if}
              <span class="min-w-0 flex-1 break-words whitespace-pre-wrap">{message.text}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
</div>

{#if pending}
  <Confirm
    title="Esto ejecuta la sentencia"
    message={pending.warning}
    confirmLabel="Ejecutar igual"
    onconfirm={() => {
      const target = pending;
      pending = null;
      if (target) tab.explain(target.sql, target.base, target.options);
    }}
    onclose={() => (pending = null)}
  />
{/if}
