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
    {
      key: "query",
      header: "Sentencia",
      width: 700,
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
  <div
    class="flex flex-wrap items-center gap-3 border-b border-neutral-200 px-3 py-2
           dark:border-neutral-800"
  >
    {#each TABS as [value, label] (value)}
      <button
        class="text-sm {tab === value
          ? 'font-medium text-blue-600 dark:text-blue-400'
          : 'text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-100'}"
        onclick={() => (tab = value)}
      >
        {label}
      </button>
    {/each}

    <label class="ml-auto flex items-center gap-1.5 text-xs text-neutral-500">
      Refresco
      <select
        class="field py-0.5"
        value={monitor.intervalMs}
        onchange={(event) => monitor.setInterval(Number(event.currentTarget.value))}
      >
        <option value={1000}>1 s</option>
        <option value={2000}>2 s</option>
        <option value={5000}>5 s</option>
        <option value={15000}>15 s</option>
      </select>
    </label>

    <label class="flex items-center gap-1.5 text-xs text-neutral-500">
      <input
        type="checkbox"
        checked={monitor.filter.includeIdle}
        onchange={(event) =>
          monitor.setFilter({ ...monitor.filter, includeIdle: event.currentTarget.checked })}
      />
      Inactivas
    </label>

    <label class="flex items-center gap-1.5 text-xs text-neutral-500">
      <input
        type="checkbox"
        checked={monitor.filter.includeBackground}
        onchange={(event) =>
          monitor.setFilter({ ...monitor.filter, includeBackground: event.currentTarget.checked })}
      />
      Procesos internos
    </label>
  </div>

  {#if monitor.error}
    <p
      class="border-b border-red-200 bg-red-50 px-3 py-1.5 text-sm text-red-700
             dark:border-red-900 dark:bg-red-950 dark:text-red-300"
    >
      {monitor.error}
    </p>
  {/if}

  {#if metrics}
    <div class="grid grid-cols-2 gap-2 px-3 py-2 md:grid-cols-4 xl:grid-cols-6">
      {#each [["Conexiones", `${metrics.totalConnections} / ${metrics.maxConnections}`], ["Activas", String(metrics.activeConnections)], ["Inactivas en transacción", String(metrics.idleInTransaction)], ["Esperando", String(metrics.waitingConnections)], ["Transacciones/s", decimal(metrics.transactionsPerSecond)], ["Transacción más vieja", duration(metrics.longestTransactionSeconds)]] as [label, value] (label)}
        <div class="rounded border border-neutral-200 px-3 py-2 dark:border-neutral-800">
          <div class="truncate text-xs text-neutral-500">{label}</div>
          <div class="font-mono text-lg tabular-nums">{value}</div>
        </div>
      {/each}
    </div>
  {/if}

  {#if tab === "sesiones"}
    <div class="grid grid-cols-2 gap-2 px-3 pb-2 xl:grid-cols-4">
      <Chart label="Conexiones" data={connectionsSeries} />
      <Chart label="Activas" data={activeSeries} color="#f59e0b" />
      <Chart
        label="Transacciones/s"
        data={tpsSeries}
        color="#10b981"
        formatValue={(value) => value.toFixed(1)}
      />
      <Chart
        label="Aciertos de caché"
        data={cacheSeries}
        color="#8b5cf6"
        formatValue={(value) => `${value.toFixed(1)} %`}
      />
    </div>

    {#if selected}
      <div
        class="flex flex-wrap items-center gap-2 border-y border-neutral-200 px-3 py-2 text-sm
               dark:border-neutral-800"
      >
        <span class="font-mono">PID {selected.pid}</span>
        {#if selected.isMonitor}
          <span class="text-xs text-neutral-500">Es la sesión del propio monitor.</span>
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
          <span class="text-xs text-neutral-500">
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
          ? 'text-red-600 dark:text-red-400'
          : 'text-emerald-600'}"
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
        <p class="text-sm text-neutral-500">Ninguna sesión está esperando a otra.</p>
      {:else}
        <p class="mb-2 text-xs text-neutral-500">
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
    <div class="flex items-center gap-2 border-y border-neutral-200 px-3 py-2 dark:border-neutral-800">
      <span class="text-xs text-neutral-500">
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
        <p class="p-4 text-sm text-red-600 dark:text-red-400">{statementsError}</p>
      {:else if statementsAvailable === false}
        <div class="space-y-2 p-4 text-sm text-neutral-500">
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
          rowKey={(statement) => statement.query}
          empty="Todavía no hay consultas registradas."
        />
      {/if}
    </div>
  {/if}
</div>

{#if confirming}
  <div class="fixed inset-0 z-10 flex items-center justify-center bg-black/40 p-4">
    <div class="w-full max-w-md rounded-lg bg-white p-5 shadow-xl dark:bg-neutral-900">
      <h2 class="text-base font-medium">
        {confirming.kind === "cancel" ? "Cancelar la consulta" : "Terminar la sesión"}
      </h2>
      <p class="mt-2 text-sm text-neutral-600 dark:text-neutral-300">
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
