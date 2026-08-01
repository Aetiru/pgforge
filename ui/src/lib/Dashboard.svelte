<script lang="ts">
  import type uPlot from "uplot";
  import BlockTree from "./BlockTree.svelte";
  import Chart from "./Chart.svelte";
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import MaintenanceDialog from "./MaintenanceDialog.svelte";
  import { ago, bytes, count, decimal, duration, oneLine, percent } from "./format";
  import {
    backendLocks,
    cancelBackend,
    describeError,
    hasStatementStats,
    indexStats,
    statementStats,
    tableStats,
    terminateBackend,
    type Backend,
    type IndexStat,
    type Lock,
    type StatementStat,
    type TableStat,
    type Target,
  } from "./ipc";
  import { monitor } from "./monitor.svelte";

  let { profileId }: { profileId: string } = $props();

  // Declaradas una sola vez y no en línea: pasarlas como flechas dentro del marcado les cambiaría
  // la identidad en cada muestra, y el gráfico se destruiría y volvería a crearse cada dos segundos.
  const oneDecimal = (value: number) => value.toFixed(1);
  const asPercent = (value: number) => `${value.toFixed(1)} %`;

  type Tab = "sesiones" | "bloqueos" | "tablas" | "indices" | "consultas";

  let tab = $state<Tab>("sesiones");
  let selectedPid = $state<number | null>(null);
  let locks = $state<Lock[]>([]);
  let actionMessage = $state<string | null>(null);
  let actionFailed = $state(false);
  let confirming = $state<{ pid: number; kind: "cancel" | "terminate" } | null>(null);

  let tables = $state<TableStat[]>([]);
  let indexes = $state<IndexStat[]>([]);
  let statements = $state<StatementStat[]>([]);
  let statementsAvailable = $state<boolean | null>(null);
  let statementsError = $state<string | null>(null);
  let selectedTable = $state<TableStat | null>(null);
  let maintenanceTarget = $state<Target | null>(null);

  $effect(() => {
    monitor.start(profileId);
    return () => {
      monitor.stop();
    };
  });

  $effect(() => monitor.watchVisibility());

  const snapshot = $derived(monitor.snapshot);
  const metrics = $derived(snapshot?.metrics ?? null);
  const backends = $derived(snapshot?.backends ?? []);
  const selected = $derived(backends.find((backend) => backend.pid === selectedPid) ?? null);

  /**
   * Los indicadores que se resaltan son los que piden una acción: conexiones cerca del techo,
   * transacciones abiertas sin actividad y sesiones esperando a otra. El resto informa.
   */
  const tiles = $derived.by(() => {
    if (!metrics) return [];
    const nearLimit =
      metrics.maxConnections > 0 && metrics.totalConnections / metrics.maxConnections > 0.8;

    return [
      {
        label: "Conexiones",
        value: `${metrics.totalConnections} / ${metrics.maxConnections}`,
        tone: nearLimit ? "text-rose-600 dark:text-rose-400" : undefined,
      },
      { label: "Activas", value: String(metrics.activeConnections) },
      {
        label: "Inactivas en transacción",
        value: String(metrics.idleInTransaction),
        tone: metrics.idleInTransaction > 0 ? "text-amber-600 dark:text-amber-400" : undefined,
      },
      {
        label: "Esperando",
        value: String(metrics.waitingConnections),
        tone: metrics.waitingConnections > 0 ? "text-rose-600 dark:text-rose-400" : undefined,
      },
      { label: "Transacciones/s", value: decimal(metrics.transactionsPerSecond) },
      { label: "Transacción más vieja", value: duration(metrics.longestTransactionSeconds) },
    ];
  });

  const times = $derived(monitor.history.map((sample) => sample.time));
  const connectionsSeries = $derived([
    times,
    monitor.history.map((sample) => sample.connections),
  ] as uPlot.AlignedData);
  const activeSeries = $derived([
    times,
    monitor.history.map((sample) => sample.active),
  ] as uPlot.AlignedData);
  const tpsSeries = $derived([
    times,
    monitor.history.map((sample) => sample.transactionsPerSecond),
  ] as uPlot.AlignedData);
  const cacheSeries = $derived([
    times,
    monitor.history.map((sample) =>
      sample.cacheHitRatio === null ? null : sample.cacheHitRatio * 100,
    ),
  ] as uPlot.AlignedData);

  // Los detalles de candados se piden solo para la sesión elegida: traerlos para todas en cada
  // ciclo sería una consulta pesada sin que nadie los mire.
  $effect(() => {
    const pid = selectedPid;
    if (pid === null) {
      locks = [];
      return;
    }
    let cancelled = false;
    backendLocks(profileId, pid)
      .then((result) => {
        if (!cancelled) locks = result;
      })
      .catch(() => {
        if (!cancelled) locks = [];
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const current = tab;
    if (current === "tablas") {
      tableStats(profileId)
        .then((result) => (tables = result))
        .catch((error) => (actionMessage = describeError(error)));
    } else if (current === "indices") {
      indexStats(profileId)
        .then((result) => (indexes = result))
        .catch((error) => (actionMessage = describeError(error)));
    } else if (current === "consultas") {
      hasStatementStats(profileId)
        .then(async (available) => {
          statementsAvailable = available;
          if (available) statements = await statementStats(profileId);
        })
        .catch((error) => (statementsError = describeError(error)));
    }
  });

  async function act(pid: number, kind: "cancel" | "terminate") {
    confirming = null;
    try {
      const done =
        kind === "cancel"
          ? await cancelBackend(profileId, pid)
          : await terminateBackend(profileId, pid);
      actionFailed = !done;
      actionMessage = done
        ? kind === "cancel"
          ? `Se pidió cancelar la consulta del PID ${pid}.`
          : `Se terminó la sesión ${pid}.`
        : `El PID ${pid} ya no existe.`;
    } catch (error) {
      actionFailed = true;
      actionMessage = describeError(error);
    }
  }

  const backendColumns: Column<Backend>[] = [
    { key: "pid", header: "PID", width: 64, align: "right", value: (b) => String(b.pid) },
    {
      key: "state",
      header: "Estado",
      width: 130,
      value: (b) => b.state ?? "—",
      tone: (b) =>
        b.state?.startsWith("idle in transaction")
          ? "text-amber-600 dark:text-amber-400"
          : undefined,
    },
    {
      key: "duration",
      header: "Consulta",
      width: 90,
      align: "right",
      value: (b) => duration(b.querySeconds),
    },
    {
      key: "xact",
      header: "Transacción",
      width: 100,
      align: "right",
      value: (b) => duration(b.transactionSeconds),
    },
    {
      key: "blocked",
      header: "Bloqueada por",
      width: 110,
      value: (b) => (b.blockedBy.length ? b.blockedBy.join(", ") : "—"),
      tone: (b) => (b.blockedBy.length ? "text-rose-600 dark:text-rose-400" : undefined),
    },
    {
      key: "wait",
      header: "Espera",
      width: 150,
      value: (b) => (b.waitEventType ? `${b.waitEventType}: ${b.waitEvent ?? ""}` : "—"),
    },
    { key: "database", header: "Base", width: 110, value: (b) => b.database ?? "—" },
    { key: "user", header: "Usuario", width: 110, value: (b) => b.user ?? "—" },
    {
      key: "app",
      header: "Aplicación",
      width: 140,
      value: (b) => b.applicationName || "—",
    },
    { key: "client", header: "Cliente", width: 120, value: (b) => b.clientAddr ?? "local" },
    {
      key: "query",
      header: "Sentencia",
      width: 700,
      value: (b) => oneLine(b.query, 400),
      title: (b) => b.query ?? undefined,
    },
  ];

  const tableColumns: Column<TableStat>[] = [
    { key: "schema", header: "Esquema", width: 130, value: (t) => t.schema },
    { key: "table", header: "Tabla", width: 200, value: (t) => t.table },
    { key: "live", header: "Filas vivas", width: 110, align: "right", value: (t) => count(t.liveTuples) },
    { key: "dead", header: "Muertas", width: 100, align: "right", value: (t) => count(t.deadTuples) },
    {
      key: "ratio",
      header: "% muertas (est.)",
      width: 120,
      align: "right",
      value: (t) => percent(t.deadRatio),
      tone: (t) =>
        (t.deadRatio ?? 0) > 0.2 ? "text-amber-600 dark:text-amber-400" : undefined,
    },
    { key: "total", header: "Tamaño", width: 100, align: "right", value: (t) => bytes(t.totalBytes) },
    { key: "idx", header: "Índices", width: 100, align: "right", value: (t) => bytes(t.indexBytes) },
    { key: "seq", header: "Seq scans", width: 100, align: "right", value: (t) => count(t.sequentialScans) },
    { key: "iscan", header: "Idx scans", width: 100, align: "right", value: (t) => count(t.indexScans) },
    {
      key: "vac",
      header: "Último autovacuum",
      width: 160,
      value: (t) => ago(t.lastAutovacuumSeconds),
    },
  ];

  const indexColumns: Column<IndexStat>[] = [
    { key: "schema", header: "Esquema", width: 130, value: (i) => i.schema },
    { key: "table", header: "Tabla", width: 200, value: (i) => i.table },
    { key: "index", header: "Índice", width: 260, value: (i) => i.index },
    { key: "scans", header: "Usos", width: 100, align: "right", value: (i) => count(i.scans) },
    { key: "size", header: "Tamaño", width: 100, align: "right", value: (i) => bytes(i.bytes) },
    {
      key: "kind",
      header: "Tipo",
      width: 120,
      value: (i) => (i.isPrimary ? "primaria" : i.isUnique ? "única" : "secundario"),
    },
    {
      key: "state",
      header: "Observación",
      width: 200,
      value: (i) =>
        !i.isValid
          ? "INVÁLIDO: hay que reconstruirlo"
          : i.scans === 0 && !i.isUnique && !i.isPrimary
            ? "nunca se usó"
            : "",
      tone: (i) =>
        !i.isValid
          ? "text-rose-600 dark:text-rose-400"
          : i.scans === 0 && !i.isUnique && !i.isPrimary
            ? "text-amber-600 dark:text-amber-400"
            : undefined,
    },
  ];

  const statementColumns: Column<StatementStat>[] = [
    { key: "database", header: "Base", width: 120, value: (s) => s.database ?? "—" },
    { key: "user", header: "Usuario", width: 110, value: (s) => s.user ?? "—" },
    {
      key: "query",
      header: "Sentencia",
      width: 620,
      value: (s) => oneLine(s.query, 400),
      title: (s) => s.query,
    },
    { key: "calls", header: "Llamadas", width: 110, align: "right", value: (s) => count(s.calls) },
    { key: "total", header: "Total", width: 110, align: "right", value: (s) => duration(s.totalMs / 1000) },
    { key: "mean", header: "Media", width: 110, align: "right", value: (s) => duration(s.meanMs / 1000) },
    { key: "rows", header: "Filas", width: 110, align: "right", value: (s) => count(s.rows) },
  ];

  const TABS: [Tab, string][] = [
    ["sesiones", "Sesiones"],
    ["bloqueos", "Bloqueos"],
    ["tablas", "Tablas"],
    ["indices", "Índices"],
    ["consultas", "Consultas lentas"],
  ];
</script>

<div class="flex h-full flex-col">
  <div class="divider-b flex flex-wrap items-center gap-3 px-3 py-2">
    <div class="seg" role="tablist">
      {#each TABS as [value, label] (value)}
        <button class="seg-item" role="tab" aria-selected={tab === value} onclick={() => (tab = value)}>
          {label}
        </button>
      {/each}
    </div>

    <label class="check ml-auto">
      <input
        type="checkbox"
        checked={monitor.filter.includeIdle}
        onchange={(event) =>
          monitor.setFilter({ ...monitor.filter, includeIdle: event.currentTarget.checked })}
      />
      Inactivas
    </label>

    <label class="check">
      <input
        type="checkbox"
        checked={monitor.filter.includeBackground}
        onchange={(event) =>
          monitor.setFilter({ ...monitor.filter, includeBackground: event.currentTarget.checked })}
      />
      Procesos internos
    </label>

    <label class="check">
      Refresco
      <select
        class="field py-0.5 text-xs"
        value={monitor.intervalMs}
        onchange={(event) => monitor.setInterval(Number(event.currentTarget.value))}
      >
        <option value={1000}>1 s</option>
        <option value={2000}>2 s</option>
        <option value={5000}>5 s</option>
        <option value={15000}>15 s</option>
      </select>
    </label>
  </div>

  {#if monitor.error}
    <p
      class="border-b border-rose-200 bg-rose-50 px-3 py-1.5 text-sm text-rose-700
             dark:border-rose-900 dark:bg-rose-950 dark:text-rose-300"
    >
      {monitor.error}
    </p>
  {/if}

  {#if metrics}
    <div class="grid grid-cols-2 gap-2 px-3 py-3 md:grid-cols-3 xl:grid-cols-6">
      {#each tiles as tile (tile.label)}
        <div class="card px-3 py-2">
          <div class="truncate text-xs muted">{tile.label}</div>
          <div class="font-mono text-xl tabular-nums {tile.tone ?? ''}">{tile.value}</div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="flex items-center gap-2 px-3 py-3 text-sm muted">
      <span class="spinner"></span> Tomando la primera muestra…
    </div>
  {/if}

  {#if tab === "sesiones"}
    <div class="grid grid-cols-2 gap-2 px-3 pb-2 xl:grid-cols-4">
      <Chart label="Conexiones" data={connectionsSeries} />
      <Chart label="Activas" data={activeSeries} color="#f59e0b" />
      <Chart label="Transacciones/s" data={tpsSeries} color="#10b981" formatValue={oneDecimal} />
      <Chart
        label="Aciertos de caché"
        data={cacheSeries}
        color="#8b5cf6"
        formatValue={asPercent}
        formatTick={oneDecimal}
      />
    </div>

    {#if selected}
      <div class="divider-t divider-b flex flex-wrap items-center gap-2 px-3 py-2 text-sm">
        <span class="tag tag-neutral font-mono">PID {selected.pid}</span>
        {#if selected.isMonitor}
          <span class="text-xs muted">Es la sesión del propio monitor.</span>
        {:else}
          <button class="btn" onclick={() => (confirming = { pid: selected.pid, kind: "cancel" })}>
            Cancelar consulta
          </button>
          <button
            class="btn"
            onclick={() => (confirming = { pid: selected.pid, kind: "terminate" })}
          >
            Terminar sesión
          </button>
        {/if}

        {#if locks.length > 0}
          <span class="truncate text-xs muted">
            Candados: {locks
              .map((lock) => `${lock.mode}${lock.granted ? "" : " (esperando)"}`)
              .join(", ")}
          </span>
        {/if}
      </div>
    {/if}

    {#if actionMessage}
      <p
        class="px-3 py-1.5 text-sm {actionFailed
          ? 'text-rose-600 dark:text-rose-400'
          : 'text-emerald-600 dark:text-emerald-400'}"
      >
        {actionMessage}
      </p>
    {/if}

    <div class="min-h-0 flex-1">
      <DataGrid
        columns={backendColumns}
        rows={backends}
        rowKey={(backend) => backend.pid}
        selectedKey={selectedPid}
        onselect={(backend) => (selectedPid = backend.pid)}
        empty="No hay sesiones que cumplan el filtro."
      />
    </div>
  {:else if tab === "bloqueos"}
    <div class="min-h-0 flex-1 overflow-auto px-3 py-2">
      {#if !snapshot || snapshot.blocking.length === 0}
        <p class="text-sm text-zinc-500">Ninguna sesión está esperando a otra.</p>
      {:else}
        <p class="mb-2 text-xs text-zinc-500">
          La sesión de arriba de cada rama es la que hay que resolver: las de abajo esperan por
          ella.
        </p>
        {#each snapshot.blocking as node (node.pid)}
          <BlockTree
            {node}
            onselect={(pid) => {
              selectedPid = pid;
              tab = "sesiones";
            }}
          />
        {/each}
      {/if}
    </div>
  {:else if tab === "tablas"}
    <div class="divider-t divider-b flex items-center gap-2 px-3 py-2">
      <span class="text-xs muted">
        La proporción de tuplas muertas es una estimación sobre los contadores de estadísticas, no
        una medición del espacio desperdiciado.
      </span>
      <button
        class="btn ml-auto"
        disabled={!selectedTable}
        onclick={() =>
          selectedTable &&
          (maintenanceTarget = {
            kind: "table",
            schema: selectedTable.schema,
            name: selectedTable.table,
          })}
      >
        Mantenimiento
      </button>
    </div>
    <div class="min-h-0 flex-1">
      <DataGrid
        columns={tableColumns}
        rows={tables}
        rowKey={(table) => `${table.schema}.${table.table}`}
        selectedKey={selectedTable ? `${selectedTable.schema}.${selectedTable.table}` : null}
        onselect={(table) => (selectedTable = table)}
        empty="No hay estadísticas de tablas en esta base."
      />
    </div>
  {:else if tab === "indices"}
    <div class="min-h-0 flex-1">
      <DataGrid
        columns={indexColumns}
        rows={indexes}
        rowKey={(index) => `${index.schema}.${index.index}`}
        empty="No hay estadísticas de índices en esta base."
      />
    </div>
  {:else}
    <div class="min-h-0 flex-1">
      {#if statementsError}
        <p class="p-4 text-sm text-rose-600 dark:text-rose-400">{statementsError}</p>
      {:else if statementsAvailable === false}
        <div class="space-y-2 p-4 text-sm text-zinc-500">
          <p>La extensión <code>pg_stat_statements</code> no está instalada en esta base.</p>
          <p>
            Para habilitarla hay que agregarla a <code>shared_preload_libraries</code>, reiniciar el
            servidor y ejecutar <code>CREATE EXTENSION pg_stat_statements;</code>.
          </p>
        </div>
      {:else}
        <DataGrid
          columns={statementColumns}
          rows={statements}
          rowKey={(statement) =>
            `${statement.database}/${statement.user}/${statement.queryId ?? statement.query}`}
          empty="Todavía no hay consultas registradas."
        />
      {/if}
    </div>
  {/if}
</div>

{#if confirming}
  <div class="fixed inset-0 z-10 flex items-center justify-center bg-black/40 p-4">
    <div class="w-full max-w-md rounded-lg bg-white p-5 shadow-xl dark:bg-zinc-900">
      <h2 class="text-base font-medium">
        {confirming.kind === "cancel" ? "Cancelar la consulta" : "Terminar la sesión"}
      </h2>
      <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">
        {#if confirming.kind === "cancel"}
          Se le pide al servidor que aborte la consulta del PID {confirming.pid}. La sesión sigue
          conectada y su transacción queda abierta pero abortada.
        {:else}
          Se cierra la sesión {confirming.pid} por completo: su transacción se revierte y el cliente
          pierde la conexión sin aviso.
        {/if}
      </p>
      <div class="mt-4 flex justify-end gap-2">
        <button class="btn" onclick={() => (confirming = null)}>No</button>
        <button
          class="btn btn-primary"
          onclick={() => confirming && act(confirming.pid, confirming.kind)}
        >
          {confirming.kind === "cancel" ? "Cancelar la consulta" : "Terminar la sesión"}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if maintenanceTarget}
  <MaintenanceDialog
    {profileId}
    target={maintenanceTarget}
    onclose={() => (maintenanceTarget = null)}
  />
{/if}
