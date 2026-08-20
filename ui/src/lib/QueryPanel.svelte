<script lang="ts">
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import ExportDialog from "./ExportDialog.svelte";
  import { environmentOf, isReadOnly } from "./access.svelte";
  import { envLook, READ_ONLY_LOOK } from "./badges";
  import HistoryPanel from "./HistoryPanel.svelte";
  import Icon from "./Icon.svelte";
  import IndexDialog from "./IndexDialog.svelte";
  import PlanAdvice from "./PlanAdvice.svelte";
  import PlanTree from "./PlanTree.svelte";
  import ResultGrid from "./ResultGrid.svelte";
  import SaveQueryDialog from "./SaveQueryDialog.svelte";
  import SavedPanel from "./SavedPanel.svelte";
  import FontSize from "./FontSize.svelte";
  import SqlEditor from "./SqlEditor.svelte";
  import { editorSplit } from "./editor.svelte";
  import { PAGE_SIZES, paging } from "./paging.svelte";
  import { count, decimal } from "./format";
  import { planText } from "./plan-text";
  import { saveQueryTab, type QueryTab } from "./query.svelte";
  import {
    dataShapeNamed,
    describeError,
    explainWarning,
    statementAtCursor,
    treeChildren,
    type ExplainOptions,
    type IndexTarget,
    type TableColumn,
  } from "./ipc";

  let { tab }: { tab: QueryTab } = $props();

  /** Qué se acaba de copiar del plan, para el «Copiado» del botón. */
  let planCopied = $state<"text" | "json" | null>(null);

  /**
   * El texto se arma acá con lo que ya está en pantalla y el JSON es el que mandó el servidor.
   * Volver a pedirle el plan en formato texto significaría, con ANALYZE, ejecutar la consulta otra
   * vez; y reconstruir el JSON desde el árbol sería inventar los campos que no se muestran.
   */
  async function copyPlan(what: "text" | "json") {
    if (!tab.plan) return;
    const text = what === "text" ? planText(tab.plan) : tab.plan.json;
    await navigator.clipboard.writeText(text).catch(() => {});
    planCopied = what;
    setTimeout(() => (planCopied = planCopied === what ? null : planCopied), 1500);
  }

  /**
   * El índice que se está por crear desde una sugerencia del plan.
   *
   * Las columnas de la tabla se piden al abrir: la sugerencia sabe cuáles indexar, pero el diálogo
   * muestra todas para poder agregar una más antes de crearlo.
   */
  let newIndex = $state<{
    target: IndexTarget;
    columns: TableColumn[];
  } | null>(null);

  async function openIndexDialog(target: IndexTarget) {
    try {
      const shape = await dataShapeNamed(tab.profileId, target.schema, target.table, tab.database);
      newIndex = { target, columns: shape.columns };
    } catch (error) {
      tab.log("error", describeError(error));
    }
  }

  let exportOpen = $state(false);
  let saveOpen = $state(false);
  /** Se incrementa al guardar, para que el panel de guardadas relea sin volver a montarse. */
  let savedStamp = $state(0);
  let editor = $state<ReturnType<typeof SqlEditor> | null>(null);
  let databases = $state<string[]>([]);
  let switching = $state<string | null>(null);
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

  /**
   * Dónde está el cursor se le pregunta al editor en el momento del clic.
   *
   * Guardarlo en el panel cada vez que el editor avisaba dejaba a los botones ejecutando la
   * sentencia de la vez anterior: el atajo lo actualizaba, pero llegar al botón con el mouse no.
   */
  function here() {
    return editor?.selection() ?? { text: "", cursor: 0 };
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
    const initial = editorSplit.height;

    const move = (moved: MouseEvent) => {
      editorSplit.set(initial + moved.clientY - origin);
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

  // `verbose` va siempre y no es una preferencia: es lo único que hace que el plan traiga el esquema
  // de cada relación, y sin eso el `CREATE INDEX` que se sugiere queda sin calificar —y copiado a
  // otra sesión podría caer sobre otra tabla que se llame igual—. Lo que agrega de más (la lista de
  // columnas de salida) no se dibuja; sí viaja en el JSON, que es donde suma.
  const analyze: ExplainOptions = { analyze: true, buffers: true, verbose: true };
  const estimate: ExplainOptions = { analyze: false, buffers: false, verbose: true };

  // Las bases del servidor, para poder apuntar la pestaña a otra sin abrir una nueva. Se piden una
  // sola vez por pestaña: crear o borrar bases mientras se escribe una consulta no es lo normal.
  $effect(() => {
    const id = tab.profileId;
    let cancelled = false;
    treeChildren(id, null, { showSystemSchemas: false })
      .then((nodes) => {
        if (cancelled) return;
        databases = nodes.filter((node) => node.kind === "database").map((node) => node.label);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  async function switchDatabase(database: string) {
    // Cambiar de base cierra la sesión, y con ella se revierte lo que esté sin confirmar.
    if (tab.txStatus !== "idle") {
      switching = database;
      return;
    }
    await tab.switchDatabase(database);
  }

  const VIEWS = [
    { value: "rows", label: "Resultados" },
    { value: "plan", label: "Plan" },
    { value: "messages", label: "Mensajes" },
    { value: "history", label: "Historial" },
    { value: "saved", label: "Guardadas" },
  ] as const;
</script>

<div class="flex h-full flex-col">
  <!--
    Íconos y no palabras: la barra tenía nueve controles con texto y en una ventana angosta se
    partía en dos líneas, comiéndose el alto del editor. «Ejecutar» conserva la etiqueta porque es
    la acción que se busca sin mirar; el resto la lleva en el `title`.
  -->
  <header class="toolbar">
    {#if tab.running}
      <button class="btn btn-danger" title="Cancela la consulta en curso" onclick={() => tab.cancel()}>
        <Icon name="close" size={12} />
        Cancelar
      </button>
    {:else}
      <button
        class="btn btn-primary"
        disabled={tab.tabId === null}
        title="Ejecuta la selección, o la sentencia donde está el cursor (Ctrl+Enter)"
        onclick={() => {
          const { text, cursor } = here();
          run(text, cursor);
        }}
      >
        <Icon name="play" size={12} />
        Ejecutar
        <span class="kbd border-white/30 bg-white/15 text-white/80">Ctrl+↵</span>
      </button>
      <button
        class="btn btn-icon"
        disabled={tab.tabId === null}
        aria-label="Ejecutar el script entero"
        title="Ejecuta todas las sentencias del editor (Ctrl+Mayús+Enter)"
        onclick={() => tab.run(tab.sql)}
      >
        <Icon name="sql" size={14} />
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
      class="btn btn-icon"
      disabled={tab.tabId === null || tab.running || tab.txStatus === "idle"}
      aria-label="Commit"
      title="Confirma la transacción abierta en esta pestaña"
      onclick={() => tab.commit()}
    >
      <Icon name="check" size={14} />
    </button>
    <button
      class="btn btn-danger btn-icon"
      disabled={tab.tabId === null || tab.running || tab.txStatus === "idle"}
      aria-label="Rollback"
      title="Descarta todo lo hecho desde que se abrió la transacción"
      onclick={() => tab.rollback()}
    >
      <Icon name="undo" size={14} />
    </button>

    <span class="toolbar-sep"></span>

    <button
      class="btn btn-icon"
      disabled={tab.tabId === null || tab.running}
      aria-label="Explicar"
      title="Muestra el plan estimado sin ejecutar la consulta"
      onclick={() => {
        const { text, cursor } = here();
        explain(text, cursor, estimate);
      }}
    >
      <Icon name="compass" size={14} />
    </button>
    <button
      class="btn btn-icon"
      disabled={tab.tabId === null || tab.running}
      aria-label="Explicar y medir"
      title="Ejecuta la consulta y muestra los tiempos reales"
      onclick={() => {
        const { text, cursor } = here();
        explain(text, cursor, analyze);
      }}
    >
      <Icon name="gauge" size={14} />
    </button>

    <span class="toolbar-sep"></span>

    <button
      class="btn btn-icon"
      aria-label="Guardar como archivo .sql"
      title={tab.filePath
        ? `Guarda en ${tab.filePath} (Ctrl+S; Ctrl+Mayús+S para elegir otro archivo)`
        : "Guarda el texto como archivo .sql (Ctrl+S)"}
      onclick={() => saveQueryTab(tab, false)}
    >
      <Icon name="save" size={14} />
    </button>

    <!-- Guardar con nombre no es guardar en un archivo: queda adentro de la aplicación, en la
         pestaña «Guardadas», y no depende de acordarse dónde se dejó el .sql. -->
    <button
      class="btn btn-icon"
      aria-label="Guardar la consulta con un nombre"
      title={tab.savedName
        ? `Guarda los cambios en «${tab.savedName}»`
        : "Guarda la consulta con un nombre para volver a abrirla"}
      onclick={() => (saveOpen = true)}
    >
      <Icon name="star" size={14} />
    </button>

    <span class="toolbar-sep"></span>

    <FontSize />

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
      <!-- Contra qué base corre se elige acá y no cerrando la pestaña para abrir otra: es la misma
           consulta contra otra base, que es como se prueba algo entre desarrollo y producción. -->
      <label class="flex items-center gap-1" title="Base sobre la que corre esta pestaña">
        <Icon name="database" size={11} />
        <select
          class="field max-w-44 py-0.5 text-xs"
          value={tab.database}
          disabled={tab.running || tab.opening}
          onchange={(event) => {
            // El desplegable vuelve a lo que hay hasta que el cambio salga bien: si falla, o si la
            // confirmación por la transacción abierta se cancela, mostraría una base que no es.
            const chosen = event.currentTarget.value;
            event.currentTarget.value = tab.database;
            switchDatabase(chosen);
          }}
        >
          {#if !databases.includes(tab.database)}
            <option value={tab.database}>{tab.database}</option>
          {/if}
          {#each databases as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
      </label>
    </span>
  </header>

  <div class="min-h-0 shrink-0 overflow-hidden" style="height: {editorSplit.height}px">
    <SqlEditor
      bind:this={editor}
      bind:value={tab.sql}
      schema={tab.schema}
      relations={tab.relations}
      errorMark={tab.errorMark}
      onrun={(selection, cursor) => run(selection, cursor)}
      onrunScript={() => tab.run(tab.sql)}
      oncancel={() => tab.cancel()}
      onsave={(askPath) => saveQueryTab(tab, askPath)}
    />
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="group relative h-px shrink-0 bg-zinc-200 dark:bg-zinc-800"
    onmousedown={startResize}
    ondblclick={() => editorSplit.reset()}
    title="Arrastrá para cambiar la altura del editor; doble clic para restablecerla. Vale para
           todas las pestañas de consulta y se recuerda."
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

      <!-- Cuántas filas se traen de una. Es la misma preferencia que usa la grilla de datos para su
           tanda: acá no hay scroll que pida la siguiente, así que sube el techo y se vuelve a
           ejecutar. -->
      <!-- Los tipos no vienen con las filas: pedirlos cuesta preparar la sentencia de nuevo en el
           servidor, así que es un interruptor y no algo que pase siempre. -->
      <label
        class="ml-auto check"
        title="Muestra el tipo de cada columna; se le pregunta al servidor sin ejecutar de nuevo"
      >
        <input
          type="checkbox"
          checked={tab.showTypes}
          disabled={tab.running}
          onchange={(event) => tab.setShowTypes(event.currentTarget.checked)}
        />
        Tipos
      </label>

      <label
        class="flex items-center gap-1 text-xs muted"
        title="Máximo de filas que se traen; para ver más, subilo y volvé a ejecutar"
      >
        <span>Máx.</span>
        <select
          class="field py-0.5 text-xs"
          value={paging.size}
          onchange={(event) => paging.set(Number(event.currentTarget.value))}
        >
          {#each PAGE_SIZES as size (size)}
            <option value={size}>{size}</option>
          {/each}
        </select>
      </label>

      <!--
        Exportar no manda las filas que están en pantalla: vuelve a correr la consulta con un
        `COPY … TO STDOUT`, así el archivo tiene todas las filas y no las que entraron en el techo.
        Por eso solo se ofrece cuando se ejecutó una sola sentencia: un script entero no es una
        consulta que `COPY` pueda envolver.
      -->
      {#if withRows}
        <button
          class="btn btn-sm"
          disabled={tab.results.length !== 1 || tab.ranSql.trim() === ""}
          title={tab.results.length !== 1
            ? "El script devolvió varios resultados: ejecutá sola la consulta que querés exportar"
            : "Exporta todas las filas de la consulta, no solo las que se muestran"}
          onclick={() => (exportOpen = true)}
        >
          <Icon name="download" size={11} />
          Exportar
        </button>

        <span class="flex items-center gap-2 text-xs muted">
          {#if withRows.truncated}
            <span
              class="tag tag-warn"
              title="La consulta devolvió más filas de las que entran en el máximo elegido"
            >
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

    <div class="relative min-h-0 flex-1">
      <!--
        Mientras corre, el estado vacío decía «todavía no ejecutaste nada», que es exactamente lo
        contrario de lo que está pasando. Va por encima y no en lugar de la grilla: al volver a
        ejecutar, tapar el resultado anterior con una pantalla vacía hace parpadear todo.
      -->
      {#if tab.running}
        <div
          class="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3
                 bg-white/70 backdrop-blur-[1px] dark:bg-zinc-950/70"
        >
          <span class="spinner size-6 border-2"></span>
          <p class="text-sm muted">Ejecutando…</p>
          <button class="btn btn-danger btn-sm" onclick={() => tab.cancel()}>
            <Icon name="close" size={11} />
            Cancelar
          </button>
        </div>
      {/if}

      {#if tab.view === "rows"}
        {#if withRows}
          {#key result}
            <ResultGrid
              columns={withRows.columns}
              rows={withRows.rows}
              types={tab.showTypes ? tab.columnTypes : null}
            />
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
            <div class="mb-2 flex items-center gap-1.5">
              <span class="text-xs font-medium">
                {tab.plan.advice.length > 0
                  ? `${tab.plan.advice.length} ${tab.plan.advice.length === 1 ? "sugerencia" : "sugerencias"}`
                  : "Plan"}
              </span>
              <span class="ml-auto"></span>
              <button
                class="btn btn-sm"
                title="Copia el plan como texto, con la forma de psql"
                onclick={() => copyPlan("text")}
              >
                <Icon name={planCopied === "text" ? "check" : "copy"} size={11} />
                {planCopied === "text" ? "Copiado" : "Copiar texto"}
              </button>
              <button
                class="btn btn-sm"
                title="Copia el JSON tal como lo devolvió el servidor, para pegarlo en un visor de planes"
                onclick={() => copyPlan("json")}
              >
                <Icon name={planCopied === "json" ? "check" : "copy"} size={11} />
                {planCopied === "json" ? "Copiado" : "Copiar JSON"}
              </button>
            </div>

            <div class="mb-3">
              <PlanAdvice
                advice={tab.plan.advice}
                analyzed={tab.plan.analyzed}
                oncreateIndex={openIndexDialog}
              />
            </div>

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
      {:else if tab.view === "saved"}
        <SavedPanel reload={savedStamp} onpick={(saved) => tab.applySaved(saved)} />
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

{#if exportOpen}
  <ExportDialog
    profileId={tab.profileId}
    database={tab.database}
    source={{ kind: "query", sql: tab.ranSql }}
    onclose={() => (exportOpen = false)}
  />
{/if}

{#if saveOpen}
  <SaveQueryDialog
    {tab}
    onsaved={() => (savedStamp += 1)}
    onclose={() => (saveOpen = false)}
  />
{/if}

{#if switching}
  <Confirm
    title="Hay una transacción abierta"
    message="Cambiar de base cierra la sesión de esta pestaña, y con ella se revierte todo lo que no
             se haya confirmado. El texto del editor queda como está."
    confirmLabel="Cambiar igual"
    onconfirm={() => {
      const database = switching;
      switching = null;
      if (database) tab.switchDatabase(database);
    }}
    onclose={() => (switching = null)}
  />
{/if}

{#if newIndex}
  <IndexDialog
    profileId={tab.profileId}
    database={tab.database}
    schema={newIndex.target.schema}
    table={newIndex.target.table}
    columns={newIndex.columns}
    initialColumns={newIndex.target.columns}
    onclose={() => (newIndex = null)}
    oncreated={() => tab.log("info", "El índice quedó creado.")}
  />
{/if}

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
