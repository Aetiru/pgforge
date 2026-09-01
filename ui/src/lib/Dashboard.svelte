<script lang="ts">
  import type uPlot from "uplot";
  import Alert from "./Alert.svelte";
  import BlockTree from "./BlockTree.svelte";
  import Chart from "./Chart.svelte";
  import Confirm from "./Confirm.svelte";
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import Empty from "./Empty.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import MaintenanceDialog from "./MaintenanceDialog.svelte";
  import { ago, bytes, count, decimal, duration, oneLine, percent } from "./format";
  import {
    backendLocks,
    cancelBackend,
    describeError,
    hasBloatStats,
    hasStatementStats,
    indexDrop,
    indexStats,
    redundantIndexes,
    statementStats,
    tableBloat,
    tableStats,
    terminateBackend,
    treeChildren,
    type Backend,
    type IndexStat,
    type Lock,
    type Redundancy,
    type StatementStat,
    type TableBloat,
    type TableStat,
    type Target,
  } from "./ipc";
  import { monitor } from "./monitor.svelte";
  import { confirmMutation } from "./access.svelte";
  import { openQuery } from "./query.svelte";
  import { explorer } from "./explorer.svelte";
  import { untrack } from "svelte";

  let { profileId }: { profileId: string } = $props();

  // Las estadísticas de tablas e índices son por base: hay que elegir cuál mirar cuando el servidor
  // tiene varias. Arranca en la base con la que se conectó el perfil (disponible ya), y la lista de
  // opciones se completa con las bases del servidor.
  let databases = $state<string[]>([]);
  let database = $state<string | null>(
    untrack(() => explorer.profiles.find((profile) => profile.id === profileId)?.database ?? null),
  );

  $effect(() => {
    const id = profileId;
    let cancelled = false;
    treeChildren(id, null, { showSystemSchemas: false })
      .then((nodes) => {
        if (cancelled) return;
        databases = nodes.filter((node) => node.kind === "database").map((node) => node.label);
        // Si la base con la que se conectó el perfil no aparece en la lista, se cae a la primera,
        // para que el selector nunca quede mostrando una opción vacía.
        if (!database || !databases.includes(database)) database = databases[0] ?? database;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Declaradas una sola vez y no en línea: pasarlas como flechas dentro del marcado les cambiaría
  // la identidad en cada muestra, y el gráfico se destruiría y volvería a crearse cada dos segundos.
  const oneDecimal = (value: number) => value.toFixed(1);
  const asPercent = (value: number) => `${value.toFixed(1)} %`;

  type Tab =
    | "sesiones"
    | "bloqueos"
    | "tablas"
    | "indices"
    | "duplicados"
    | "bloat"
    | "consultas";

  let tab = $state<Tab>("sesiones");
  let selectedPid = $state<number | null>(null);
  let locks = $state<Lock[]>([]);
  let actionMessage = $state<string | null>(null);
  let actionFailed = $state(false);
  let confirming = $state<{ pid: number; kind: "cancel" | "terminate" } | null>(null);

  let tables = $state<TableStat[]>([]);
  let indexes = $state<IndexStat[]>([]);
  let statements = $state<StatementStat[]>([]);
  let selectedStatement = $state<StatementStat | null>(null);
  let statementsAvailable = $state<boolean | null>(null);
  let statementsError = $state<string | null>(null);
  let bloat = $state<TableBloat[]>([]);
  let bloatAvailable = $state<boolean | null>(null);
  let bloatError = $state<string | null>(null);
  let selectedTable = $state<TableStat | null>(null);
  let selectedIndex = $state<IndexStat | null>(null);

  /** Índices que otro ya cubre, y cuál está elegido para borrar. */
  let redundant = $state<Redundancy[]>([]);
  let redundantError = $state<string | null>(null);
  let dropError = $state<string | null>(null);

  /** Un índice se identifica por esquema y nombre: el nombre solo se repite entre esquemas. */
  const keyOf = (item: Redundancy) => `${item.schema}.${item.index}`;
  let selectedRedundant = $state<Redundancy | null>(null);
  let droppingIndex = $state<Redundancy | null>(null);
  let selectedBloat = $state<TableBloat | null>(null);
  let maintenanceTarget = $state<Target | null>(null);

  $effect(() => {
    // El servidor y la base elegida disparan esto. Sin untrack, start() lee monitor.profileId (en su
    // guarda) y lo vuelve a escribir al conectar: el efecto quedaría dependiendo de un estado que él
    // mismo cambia, se reiniciaría en bucle y reconectaría sin parar —el parpadeo de "error
    // connecting to server" mientras la muestra nunca llega a estabilizarse—.
    const id = profileId;
    const db = database;
    untrack(() => monitor.start(id, db));
    return () => untrack(() => monitor.stop());
  });

  $effect(() => monitor.watchVisibility());

  /** Con estos dos permisos apagados, lo que se ve es una parte de lo que hay: conviene decirlo. */
  const limitedStats = $derived.by(() => {
    const caps = explorer.caps[profileId];
    return caps ? !caps.isSuperuser && !caps.canReadAllStats : false;
  });

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
        icon: "plug" as const,
        value: `${metrics.totalConnections} / ${metrics.maxConnections}`,
        tone: nearLimit ? "bad" : null,
        hint: nearLimit ? "cerca del máximo configurado" : null,
      },
      {
        label: "Activas",
        icon: "play" as const,
        value: String(metrics.activeConnections),
        tone: null,
        hint: null,
      },
      {
        label: "Inactivas en transacción",
        icon: "pin" as const,
        value: String(metrics.idleInTransaction),
        tone: metrics.idleInTransaction > 0 ? "warn" : null,
        hint: metrics.idleInTransaction > 0 ? "retienen candados sin trabajar" : null,
      },
      {
        label: "Esperando",
        icon: "lock" as const,
        value: String(metrics.waitingConnections),
        tone: metrics.waitingConnections > 0 ? "bad" : null,
        hint: metrics.waitingConnections > 0 ? "bloqueadas por otra sesión" : null,
      },
      {
        label: "Transacciones/s",
        icon: "gauge" as const,
        value: decimal(metrics.transactionsPerSecond),
        tone: null,
        hint: null,
      },
      {
        label: "Transacción más vieja",
        icon: "clock" as const,
        value: duration(metrics.longestTransactionSeconds),
        tone: null,
        hint: null,
      },
    ];
  });

  const TILE_TONE: Record<string, string> = {
    bad: "text-rose-600 dark:text-rose-400",
    warn: "text-amber-600 dark:text-amber-400",
  };

  const TILE_EDGE: Record<string, string> = {
    bad: "border-l-rose-500",
    warn: "border-l-amber-500",
  };

  const TILE_ICON_TONE: Record<string, string> = {
    bad: "bg-rose-100 text-rose-600 dark:bg-rose-950 dark:text-rose-400",
    warn: "bg-amber-100 text-amber-600 dark:bg-amber-950 dark:text-amber-400",
  };

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
    // Se recargan al cambiar de pestaña y cuando el monitor queda listo en otra base: leer
    // monitor.profileId/database hace que el efecto reaccione a la reapertura de la sesión (que pasa
    // por null), y saltea el hueco entre parar y arrancar en el que el monitor no está activo.
    void monitor.database;
    if (!monitor.profileId) return;
    if (current === "tablas") {
      tableStats(profileId)
        .then((result) => (tables = result))
        .catch((error) => (actionMessage = describeError(error)));
    } else if (current === "indices") {
      indexStats(profileId)
        .then((result) => (indexes = result))
        .catch((error) => (actionMessage = describeError(error)));
    } else if (current === "duplicados") {
      redundantError = null;
      dropError = null;
      redundantIndexes(profileId)
        .then((result) => (redundant = result))
        .catch((error) => (redundantError = describeError(error)));
    } else if (current === "bloat") {
      bloatError = null;
      hasBloatStats(profileId)
        .then(async (available) => {
          bloatAvailable = available;
          if (available) bloat = await tableBloat(profileId);
        })
        .catch((error) => (bloatError = describeError(error)));
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
    {
      key: "pid",
      header: "PID",
      width: 64,
      align: "right",
      value: (b) => String(b.pid),
      sort: (b) => b.pid,
    },
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
      sort: (b) => b.querySeconds ?? -1,
    },
    {
      key: "xact",
      header: "Transacción",
      width: 100,
      align: "right",
      value: (b) => duration(b.transactionSeconds),
      sort: (b) => b.transactionSeconds ?? -1,
    },
    {
      key: "blocked",
      header: "Bloqueada por",
      width: 110,
      value: (b) => (b.blockedBy.length ? b.blockedBy.join(", ") : "—"),
      tone: (b) => (b.blockedBy.length ? "text-rose-600 dark:text-rose-400" : undefined),
      sort: (b) => -b.blockedBy.length,
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
    {
      key: "live",
      header: "Filas vivas",
      width: 110,
      align: "right",
      value: (t) => count(t.liveTuples),
      sort: (t) => t.liveTuples ?? -1,
    },
    {
      key: "dead",
      header: "Muertas",
      width: 100,
      align: "right",
      value: (t) => count(t.deadTuples),
      sort: (t) => t.deadTuples ?? -1,
    },
    {
      key: "ratio",
      header: "% muertas (est.)",
      width: 120,
      align: "right",
      value: (t) => percent(t.deadRatio),
      sort: (t) => t.deadRatio ?? -1,
      tone: (t) => ((t.deadRatio ?? 0) > 0.2 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
    {
      key: "total",
      header: "Tamaño",
      width: 100,
      align: "right",
      value: (t) => bytes(t.totalBytes),
      sort: (t) => t.totalBytes ?? -1,
    },
    {
      key: "idx",
      header: "Índices",
      width: 100,
      align: "right",
      value: (t) => bytes(t.indexBytes),
      sort: (t) => t.indexBytes ?? -1,
    },
    {
      key: "seq",
      header: "Seq scans",
      width: 100,
      align: "right",
      value: (t) => count(t.sequentialScans),
      sort: (t) => t.sequentialScans ?? -1,
    },
    {
      key: "iscan",
      header: "Idx scans",
      width: 100,
      align: "right",
      value: (t) => count(t.indexScans),
      sort: (t) => t.indexScans ?? -1,
    },
    {
      key: "vac",
      header: "Último autovacuum",
      width: 160,
      value: (t) => ago(t.lastAutovacuumSeconds),
      sort: (t) => t.lastAutovacuumSeconds ?? Number.MAX_SAFE_INTEGER,
    },
  ];

  const redundantColumns: Column<Redundancy>[] = [
    { key: "schema", header: "Esquema", width: 120, value: (r) => r.schema },
    { key: "table", header: "Tabla", width: 180, value: (r) => r.table },
    { key: "index", header: "Índice que sobra", width: 240, value: (r) => r.index },
    { key: "coveredBy", header: "Ya lo cubre", width: 240, value: (r) => r.coveredBy },
    {
      key: "kind",
      header: "Por qué",
      width: 150,
      value: (r) => (r.kind === "duplicate" ? "mismas columnas" : "es el principio del otro"),
    },
    {
      key: "scans",
      header: "Usos",
      width: 90,
      align: "right",
      value: (r) => count(r.scans),
      sort: (r) => r.scans,
    },
    {
      key: "size",
      header: "Tamaño",
      width: 100,
      align: "right",
      value: (r) => bytes(r.bytes),
      sort: (r) => r.bytes,
    },
  ];

  /**
   * Borra el índice elegido. Va con CONCURRENTLY salvo que sea un índice particionado, que el
   * servidor rechaza así: lo dice `dropSql`, que es la sentencia que se mostró en la confirmación.
   */
  async function dropRedundant(target: Redundancy) {
    droppingIndex = null;
    if (!(await confirmMutation(profileId, "Se va a borrar un índice del servidor."))) return;
    try {
      const concurrently = target.dropSql.includes("CONCURRENTLY");
      await indexDrop(
        profileId,
        target.schema,
        target.index,
        false,
        concurrently,
        database ?? undefined,
      );
      // Por esquema, no por nombre suelto: el de un índice es único adentro de su esquema, así que
      // dos esquemas con el mismo idx_estado se borrarían los dos de la lista.
      redundant = redundant.filter((item) => keyOf(item) !== keyOf(target));
      if (selectedRedundant && keyOf(selectedRedundant) === keyOf(target)) selectedRedundant = null;
      dropError = null;
    } catch (error) {
      // Aparte del error de lectura: ese reemplaza la grilla entera, y un borrado rechazado —lo que
      // pasa con un índice que sostiene algo— dejaba la lista invisible hasta cambiar de pestaña.
      dropError = describeError(error);
    }
  }

  const indexColumns: Column<IndexStat>[] = [
    { key: "schema", header: "Esquema", width: 130, value: (i) => i.schema },
    { key: "table", header: "Tabla", width: 200, value: (i) => i.table },
    { key: "index", header: "Índice", width: 260, value: (i) => i.index },
    {
      key: "scans",
      header: "Usos",
      width: 100,
      align: "right",
      value: (i) => count(i.scans),
      sort: (i) => i.scans ?? -1,
    },
    {
      key: "size",
      header: "Tamaño",
      width: 100,
      align: "right",
      value: (i) => bytes(i.bytes),
      sort: (i) => i.bytes ?? -1,
    },
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
        !i.isValid ? "INVÁLIDO: hay que reconstruirlo" : i.unused ? "nunca se usó" : "",
      tone: (i) =>
        !i.isValid
          ? "text-rose-600 dark:text-rose-400"
          : i.unused
            ? "text-amber-600 dark:text-amber-400"
            : undefined,
    },
  ];

  /**
   * Abre una pestaña de consulta con este texto y lleva a ella.
   *
   * Es el puente que faltaba entre mirar y hacer: hasta ahora, para explicar la consulta más cara
   * había que seleccionarla, copiarla a mano y abrir una pestaña. No se ejecuta ni se explica sola
   * —el texto de `pg_stat_statements` viene normalizado, con `$1` en lugar de los valores, y eso el
   * servidor no lo planifica sin `PREPARE`—: queda escrito para completarlo y correrlo.
   */
  async function openInQuery(sql: string, target?: string | null) {
    try {
      const tab = await openQuery(profileId, target ?? database ?? "", "Consulta");
      tab.sql = sql;
    } catch (error) {
      actionMessage = describeError(error);
    }
  }

  const statementColumns: Column<StatementStat>[] = [
    { key: "database", header: "Base", width: 120, value: (s) => s.database ?? "—" },
    { key: "user", header: "Usuario", width: 110, value: (s) => s.user ?? "—" },
    {
      key: "query",
      header: "Sentencia",
      width: 620,
      value: (s) => oneLine(s.query, 400),
      title: (s) => s.query ?? undefined,
    },
    {
      key: "calls",
      header: "Llamadas",
      width: 110,
      align: "right",
      value: (s) => count(s.calls),
      sort: (s) => s.calls,
    },
    {
      key: "total",
      header: "Total",
      width: 110,
      align: "right",
      value: (s) => duration(s.totalMs / 1000),
      sort: (s) => s.totalMs,
    },
    {
      key: "mean",
      header: "Media",
      width: 110,
      align: "right",
      value: (s) => duration(s.meanMs / 1000),
      sort: (s) => s.meanMs,
    },
    {
      key: "rows",
      header: "Filas",
      width: 110,
      align: "right",
      value: (s) => count(s.rows),
      sort: (s) => s.rows,
    },
  ];

  const TABS: { value: Tab; label: string; icon: IconName }[] = [
    { value: "sesiones", label: "Sesiones", icon: "gauge" },
    { value: "bloqueos", label: "Bloqueos", icon: "lock" },
    { value: "tablas", label: "Tablas", icon: "table" },
    { value: "indices", label: "Índices", icon: "index" },
    { value: "duplicados", label: "Índices de más", icon: "compare" },
    { value: "bloat", label: "Bloat", icon: "warn" },
    { value: "consultas", label: "Consultas lentas", icon: "sql" },
  ];

  const bloatColumns: Column<TableBloat>[] = [
    { key: "schema", header: "Esquema", width: 130, value: (t) => t.schema },
    { key: "table", header: "Tabla", width: 200, value: (t) => t.table },
    {
      key: "total",
      header: "Tamaño",
      width: 110,
      align: "right",
      value: (t) => bytes(t.totalBytes),
      sort: (t) => t.totalBytes,
    },
    {
      key: "free",
      header: "Libre (est.)",
      width: 110,
      align: "right",
      value: (t) => bytes(t.freeBytes),
      sort: (t) => t.freeBytes,
    },
    {
      key: "freeRatio",
      header: "% libre",
      width: 100,
      align: "right",
      value: (t) => percent(t.freeRatio),
      sort: (t) => t.freeRatio,
      tone: (t) => (t.freeRatio > 0.3 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
    {
      key: "deadRatio",
      header: "% muertas",
      width: 100,
      align: "right",
      value: (t) => percent(t.deadRatio),
      sort: (t) => t.deadRatio,
      tone: (t) => (t.deadRatio > 0.2 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
  ];

  /** Cuántas sesiones esperan a otra: el número que decide si hay que mirar la pestaña. */
  const blocked = $derived(backends.filter((backend) => backend.blockedBy.length > 0).length);
</script>

<div class="flex h-full flex-col">
  <div class="toolbar">
    <div class="seg" role="tablist">
      {#each TABS as item (item.value)}
        <button
          class="seg-item"
          role="tab"
          aria-selected={tab === item.value}
          onclick={() => (tab = item.value)}
        >
          <Icon name={item.icon} size={12} />
          {item.label}
          {#if item.value === "bloqueos" && blocked > 0}
            <span class="tag tag-bad px-1 py-0 text-[10px]">{blocked}</span>
          {/if}
        </button>
      {/each}
    </div>

    {#if databases.length > 1 || database}
      <span class="toolbar-sep"></span>
    {/if}

    {#if databases.length > 1}
      <label class="check" title="Base cuyas tablas, índices y sentencias se muestran">
        Base
        <select
          class="field py-0.5 text-xs"
          value={database}
          onchange={(event) => (database = event.currentTarget.value)}
        >
          {#each databases as db (db)}
            <option value={db}>{db}</option>
          {/each}
        </select>
      </label>
    {/if}

    {#if database}
      <button
        class="btn btn-sm"
        title={`VACUUM, ANALYZE o REINDEX sobre toda la base ${database}`}
        onclick={() => (maintenanceTarget = { kind: "database", name: database! })}
      >
        <Icon name="gauge" size={12} />
        Mantenimiento de la base
      </button>
    {/if}

    <span class="ml-auto"></span>

    <label class="check">
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

    <span class="toolbar-sep"></span>

    <label class="check">
      <Icon name="refresh" size={11} />
      <select
        class="field py-0.5 text-xs"
        title="Cada cuánto se toma una muestra"
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
    <Alert tone="bad">{monitor.error}</Alert>
  {/if}

  <!-- Sin este aviso, media tabla en blanco parece un error de la aplicación y no lo que es: el
       servidor esconde el detalle de las sesiones ajenas a quien no puede verlas. -->
  {#if limitedStats}
    <Alert tone="warn">
      El rol conectado no es superusuario ni miembro de <code>pg_read_all_stats</code>: de las
      sesiones de otros usuarios solo se ve el PID, y las estadísticas por tabla e índice pueden
      venir incompletas.
    </Alert>
  {/if}

  <div class="bg-zinc-50/70 dark:bg-black/15">
    {#if metrics}
      <div
        class="grid gap-3 px-4 py-4"
        style="grid-template-columns: repeat(auto-fit, minmax(178px, 1fr))"
      >
        {#each tiles as tile (tile.label)}
          <div
            class="card flex items-start gap-3 overflow-hidden rounded-xl px-3.5 py-3
              shadow-sm transition-shadow hover:shadow-md {tile.tone
              ? TILE_EDGE[tile.tone]
              : 'border-l-4 border-l-transparent'}"
          >
            <span
              class="grid size-8 shrink-0 place-items-center rounded-lg ring-4 {tile.tone
                ? `${TILE_ICON_TONE[tile.tone]} ring-current/10`
                : 'bg-blue-50 text-blue-600 ring-blue-500/10 dark:bg-blue-950/50 dark:text-blue-400'}"
            >
              <Icon name={tile.icon} size={15} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="truncate text-[11px] font-medium tracking-wide muted" title={tile.label}>
                {tile.label}
              </div>
              <div
                class="font-mono text-xl leading-tight font-semibold tabular-nums {tile.tone
                  ? TILE_TONE[tile.tone]
                  : ''}"
              >
                {tile.value}
              </div>
              {#if tile.hint}
                <div
                  class="mt-0.5 truncate text-[10.5px] {tile.tone ? TILE_TONE[tile.tone] : 'muted'}"
                  title={tile.hint}
                >
                  {tile.hint}
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <div class="flex items-center gap-2 px-4 py-4 text-sm muted">
        <span class="spinner"></span> Tomando la primera muestra…
      </div>
    {/if}

    {#if tab === "sesiones"}
      <div class="px-4 pb-4">
        <div class="card overflow-hidden rounded-xl shadow-sm">
          <div class="card-head justify-between">
            <span class="card-title flex items-center gap-1.5">
              <span class="relative flex size-1.5">
                <span
                  class="absolute inline-flex size-full animate-ping rounded-full bg-emerald-400 opacity-75"
                ></span>
                <span class="relative inline-flex size-1.5 rounded-full bg-emerald-500"></span>
              </span>
              Actividad en vivo
            </span>
            <span class="text-[11px] muted">muestra cada {monitor.intervalMs / 1000}s</span>
          </div>
          <div class="grid grid-cols-2 divide-x divide-zinc-200 xl:grid-cols-4 dark:divide-zinc-700">
            <Chart label="Conexiones" data={connectionsSeries} />
            <Chart label="Activas" data={activeSeries} color="#f59e0b" />
            <Chart
              label="Transacciones/s"
              data={tpsSeries}
              color="#10b981"
              formatValue={oneDecimal}
            />
            <Chart
              label="Aciertos de caché"
              data={cacheSeries}
              color="#8b5cf6"
              formatValue={asPercent}
              formatTick={oneDecimal}
            />
          </div>
        </div>
      </div>
    {/if}
  </div>

  {#if tab === "sesiones"}
    {#if selected}
      <div class="px-4 pb-3">
        <div
          class="card flex flex-wrap items-center gap-3 rounded-xl border-blue-200 bg-blue-50/70 px-3.5
            py-2.5 text-sm shadow-sm dark:border-blue-900/60 dark:bg-blue-950/30"
        >
          <span
            class="grid size-8 shrink-0 place-items-center rounded-full bg-blue-600 font-mono text-[11px]
              font-bold text-white shadow-sm shadow-blue-600/30"
          >
            {selected.pid.toString().slice(-3)}
          </span>
          <div class="min-w-0">
            <div class="flex items-center gap-1.5 font-mono text-xs font-medium">
              PID {selected.pid}
            </div>
            <div class="truncate text-xs muted">
              {selected.user ?? "?"}@{selected.database ?? "?"}
              {#if selected.state}· {selected.state}{/if}
            </div>
          </div>

          {#if locks.length > 0}
            <span class="truncate text-xs muted" title="Candados que tiene o espera esta sesión">
              candados: {locks
                .map((lock) => `${lock.mode}${lock.granted ? "" : " (esperando)"}`)
                .join(", ")}
            </span>
          {/if}

          {#if selected.isMonitor}
            <span class="tag tag-neutral ml-auto">es la sesión del propio monitor</span>
          {:else}
            <span class="ml-auto flex gap-1.5">
              <!-- Mirar qué está corriendo y poder explicarlo son el mismo movimiento; hasta ahora
                   había que copiar el texto a mano de la celda. -->
              {#if selected.query}
                <button class="btn btn-sm" onclick={() => openInQuery(selected.query ?? "", selected.database)}>
                  Abrir en una consulta
                </button>
              {/if}
              <button
                class="btn btn-sm"
                onclick={() => (confirming = { pid: selected.pid, kind: "cancel" })}
              >
                Cancelar consulta
              </button>
              <button
                class="btn btn-sm btn-danger-ghost"
                onclick={() => (confirming = { pid: selected.pid, kind: "terminate" })}
              >
                Terminar sesión
              </button>
            </span>
          {/if}
        </div>
      </div>
    {/if}

    {#if actionMessage}
      <Alert tone={actionFailed ? "bad" : "ok"} onclose={() => (actionMessage = null)}>
        {actionMessage}
      </Alert>
    {/if}

    <div class="min-h-0 flex-1">
      <DataGrid
        columns={backendColumns}
        rows={backends}
        rowKey={(backend) => backend.pid}
        selectedKey={selectedPid}
        onselect={(backend) => (selectedPid = backend.pid)}
        sortable
        empty="No hay sesiones que cumplan el filtro."
      />
    </div>
  {:else if tab === "bloqueos"}
    <div class="min-h-0 flex-1 overflow-auto px-3 py-2">
      {#if !snapshot || snapshot.blocking.length === 0}
        <Empty
          icon="check"
          title="Ninguna sesión está esperando a otra"
          hint="Cuando una sesión quede bloqueada, acá aparece la cadena completa hasta la que hay que resolver."
        />
      {:else}
        <p class="mb-2 text-xs muted">
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
    <div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
      <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
        <div class="card-head justify-between">
          <span class="card-title flex items-center gap-1.5">
            <Icon name="table" size={13} />
            Estadísticas de tablas
          </span>
          <span class="tag tag-neutral font-mono">{count(tables.length)}</span>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            La proporción de tuplas muertas es una estimación sobre los contadores de estadísticas,
            no una medición del espacio desperdiciado.
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedTable}
            title={selectedTable
              ? `VACUUM, ANALYZE o REINDEX sobre ${selectedTable.schema}.${selectedTable.table}`
              : "Elegí una tabla de la lista"}
            onclick={() =>
              selectedTable &&
              (maintenanceTarget = {
                kind: "table",
                schema: selectedTable.schema,
                name: selectedTable.table,
              })}
          >
            <Icon name="gauge" size={12} />
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
            sortable
            empty="No hay estadísticas de tablas en esta base."
          />
        </div>
      </div>
    </div>
  {:else if tab === "indices"}
    <div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
      <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
        <div class="card-head justify-between">
          <span class="card-title flex items-center gap-1.5">
            <Icon name="index" size={13} />
            Estadísticas de índices
          </span>
          <span class="tag tag-neutral font-mono">{count(indexes.length)}</span>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            Un índice que nunca se usó o que quedó inválido se reconstruye con REINDEX.
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedIndex}
            title={selectedIndex
              ? `REINDEX sobre ${selectedIndex.schema}.${selectedIndex.index}`
              : "Elegí un índice de la lista"}
            onclick={() =>
              selectedIndex &&
              (maintenanceTarget = {
                kind: "index",
                schema: selectedIndex.schema,
                name: selectedIndex.index,
              })}
          >
            <Icon name="gauge" size={12} />
            Mantenimiento
          </button>
        </div>
        <div class="min-h-0 flex-1">
          <DataGrid
            columns={indexColumns}
            rows={indexes}
            rowKey={(index) => `${index.schema}.${index.index}`}
            selectedKey={selectedIndex ? `${selectedIndex.schema}.${selectedIndex.index}` : null}
            onselect={(index) => (selectedIndex = index)}
            sortable
            empty="No hay estadísticas de índices en esta base."
          />
        </div>
      </div>
    </div>
  {:else if tab === "duplicados"}
    <div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
      <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
        <div class="card-head justify-between">
          <span class="card-title flex items-center gap-1.5">
            <Icon name="compare" size={13} />
            Índices que sobran
          </span>
          <span class="tag tag-neutral font-mono">{count(redundant.length)}</span>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            Un índice que otro ya cubre ocupa disco y hace más lenta cada escritura sin acelerar
            ninguna lectura. Lo que sostiene algo —una restricción, una clave foránea, la identidad
            de réplica, el orden de un CLUSTER— nunca aparece acá.
          </span>
          <button
            class="btn btn-sm btn-danger ml-auto"
            disabled={!selectedRedundant}
            title={selectedRedundant
              ? `DROP INDEX sobre ${selectedRedundant.schema}.${selectedRedundant.index}`
              : "Elegí un índice de la lista"}
            onclick={() => (droppingIndex = selectedRedundant)}
          >
            <Icon name="trash" size={12} />
            Borrar el índice…
          </button>
        </div>
        {#if dropError}
          <Alert tone="bad">{dropError}</Alert>
        {/if}
        <div class="min-h-0 flex-1">
          {#if redundantError}
            <Alert tone="bad">{redundantError}</Alert>
          {:else}
            <DataGrid
              columns={redundantColumns}
              rows={redundant}
              rowKey={keyOf}
              selectedKey={selectedRedundant ? keyOf(selectedRedundant) : null}
              onselect={(item) => (selectedRedundant = item)}
              sortable
              empty="Ningún índice de esta base está cubierto por otro."
            />
          {/if}
        </div>
      </div>
    </div>
  {:else if tab === "bloat"}
    <div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
      <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
        <div class="card-head justify-between">
          <span class="card-title flex items-center gap-1.5">
            <Icon name="warn" size={13} />
            Bloat estimado
          </span>
          <span class="tag tag-neutral font-mono">{count(bloat.length)}</span>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            Medición aproximada con pgstattuple: el espacio libre se recupera con VACUUM FULL, que
            bloquea la tabla mientras corre.
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedBloat}
            title={selectedBloat
              ? `VACUUM sobre ${selectedBloat.schema}.${selectedBloat.table}`
              : "Elegí una tabla de la lista"}
            onclick={() =>
              selectedBloat &&
              (maintenanceTarget = {
                kind: "table",
                schema: selectedBloat.schema,
                name: selectedBloat.table,
              })}
          >
            <Icon name="gauge" size={12} />
            Mantenimiento
          </button>
        </div>
        <div class="min-h-0 flex-1">
          {#if bloatError}
            <Alert tone="bad">{bloatError}</Alert>
          {:else if bloatAvailable === false}
            <Empty
              icon="info"
              title="Falta la extensión pgstattuple"
              hint="Ejecutá CREATE EXTENSION pgstattuple; en esta base para estimar el bloat de las tablas."
            />
          {:else}
            <DataGrid
              columns={bloatColumns}
              rows={bloat}
              rowKey={(table) => `${table.schema}.${table.table}`}
              selectedKey={selectedBloat ? `${selectedBloat.schema}.${selectedBloat.table}` : null}
              onselect={(table) => (selectedBloat = table)}
              sortable
              empty="No hay tablas con bloat que estimar en esta base."
            />
          {/if}
        </div>
      </div>
    </div>
  {:else}
    <div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
      <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
        <div class="card-head justify-between">
          <span class="card-title flex items-center gap-1.5">
            <Icon name="sql" size={13} />
            Consultas más lentas
          </span>
          <span class="tag tag-neutral font-mono">{count(statements.length)}</span>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            El texto viene normalizado: los valores aparecen como $1, $2. Al abrirlo en una consulta
            hay que completarlos antes de explicar.
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedStatement?.query}
            title={selectedStatement?.query
              ? "Abre una pestaña de consulta con este texto"
              : "Elegí una consulta de la lista"}
            onclick={() =>
              selectedStatement?.query &&
              openInQuery(selectedStatement.query, selectedStatement.database)}
          >
            <Icon name="play" size={12} />
            Abrir en una consulta
          </button>
        </div>
        <div class="min-h-0 flex-1">
          {#if statementsError}
            <Alert tone="bad">{statementsError}</Alert>
          {:else if statementsAvailable === false}
            <Empty
              icon="info"
              title="Falta la extensión pg_stat_statements"
              hint="Agregala a shared_preload_libraries, reiniciá el servidor y ejecutá CREATE EXTENSION pg_stat_statements; en esta base."
            />
          {:else}
            <DataGrid
              columns={statementColumns}
              rows={statements}
              rowKey={(statement) =>
                `${statement.database}/${statement.user}/${statement.queryId ?? statement.query}`}
              selectedKey={selectedStatement
                ? `${selectedStatement.database}/${selectedStatement.user}/${selectedStatement.queryId ?? selectedStatement.query}`
                : null}
              onselect={(statement) => (selectedStatement = statement)}
              sortable
              empty="Todavía no hay consultas registradas."
            />
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

{#if confirming}
  <Confirm
    title={confirming.kind === "cancel" ? "Cancelar la consulta" : "Terminar la sesión"}
    message={confirming.kind === "cancel"
      ? `Se le pide al servidor que aborte la consulta del PID ${confirming.pid}. La sesión sigue conectada y su transacción queda abierta pero abortada.`
      : `Se cierra la sesión ${confirming.pid} por completo: su transacción se revierte y el cliente pierde la conexión sin aviso.`}
    confirmLabel={confirming.kind === "cancel" ? "Cancelar la consulta" : "Terminar la sesión"}
    onconfirm={() => confirming && act(confirming.pid, confirming.kind)}
    onclose={() => (confirming = null)}
  />
{/if}

{#if droppingIndex}
  <Confirm
    title="Borrar «{droppingIndex.index}»"
    message="{droppingIndex.coveredBy} cubre lo mismo, así que las consultas que hoy usan este índice pueden seguir usando aquel. Se ejecuta: {droppingIndex.dropSql}"
    confirmLabel="Borrar el índice"
    danger
    onconfirm={() => droppingIndex && dropRedundant(droppingIndex)}
    onclose={() => (droppingIndex = null)}
  />
{/if}

{#if maintenanceTarget}
  <MaintenanceDialog
    {profileId}
    target={maintenanceTarget}
    {database}
    onclose={() => (maintenanceTarget = null)}
  />
{/if}
