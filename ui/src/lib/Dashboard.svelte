<script lang="ts">
  import Skeleton from "./Skeleton.svelte";
  import { t } from "./i18n.svelte";
  import type uPlot from "uplot";
  import Alert from "./Alert.svelte";
  import AlertsPanel from "./AlertsPanel.svelte";
  import Chart from "./Chart.svelte";
  import Confirm from "./Confirm.svelte";
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import Empty from "./Empty.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import CachePanel from "./diagnose/CachePanel.svelte";
  import ConnectionsPanel from "./diagnose/ConnectionsPanel.svelte";
  import PreventPanel from "./diagnose/PreventPanel.svelte";
  import ReportDialog from "./diagnose/ReportDialog.svelte";
  import ServerPanel from "./diagnose/ServerPanel.svelte";
  import SpacePanel from "./diagnose/SpacePanel.svelte";
  import SessionsPanel from "./SessionsPanel.svelte";
  import { needsValues, withValuesHint } from "./statement-explain";
  import Spark from "./Spark.svelte";
  import { assessHealth, connectionLevel } from "./health";
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
    statsWindow,
    tableBloat,
    tableStats,
    terminateBackend,
    databaseNames,
    type IndexStat,
    type Lock,
    type Redundancy,
    type StatementStat,
    type StatsWindow,
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
    databaseNames(id, { showSystemSchemas: false })
      .then((names) => {
        if (cancelled) return;
        databases = names;
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
    | "tablas"
    | "indices"
    | "duplicados"
    | "bloat"
    | "consultas"
    | "prevencion"
    | "espacio"
    | "cache"
    | "conexiones"
    | "servidor";

  /** Las pestañas que traen sus propios datos (ver `diagnose/`): `loadStats` no las toca. */
  const OWN_TABS: Tab[] = ["prevencion", "espacio", "cache", "conexiones", "servidor"];
  let reporting = $state(false);

  let tab = $state<Tab>("sesiones");
  let selectedPid = $state<number | null>(null);
  let locks = $state<Lock[]>([]);
  let actionMessage = $state<string | null>(null);
  let actionFailed = $state(false);
  let confirming = $state<{ pids: number[]; kind: "cancel" | "terminate" } | null>(null);

  let tables = $state<TableStat[]>([]);
  let indexes = $state<IndexStat[]>([]);
  /** Desde cuándo cuentan los usos de `indexes`. */
  let usageWindow = $state<StatsWindow | null>(null);

  /** La frase que acompaña a «Usos», para que un 0 no se lea como «nunca en la vida». */
  const countingSince = $derived(
    !usageWindow
      ? null
      : usageWindow.resetSeconds === null
        ? t("Los usos cuentan desde que se creó la base o desde la última caída del servidor: las estadísticas nunca se reiniciaron a mano.")
        : t("Los usos cuentan desde que se reiniciaron las estadísticas, {since}: un índice que solo se usa a fin de mes puede figurar sin usos.", { since: ago(usageWindow.resetSeconds) }),
  );
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
  /** El índice de la pestaña «Índices» que se está por borrar. */
  let droppingStat = $state<IndexStat | null>(null);
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

  const snapshot = $derived(monitor.snapshot);
  const metrics = $derived(snapshot?.metrics ?? null);
  const backends = $derived(snapshot?.backends ?? []);

  /**
   * Los indicadores que se resaltan son los que piden una acción: conexiones cerca del techo,
   * transacciones abiertas sin actividad y sesiones esperando a otra. El resto informa.
   */
  const tiles = $derived.by(() => {
    if (!metrics) return [];
    const nearLimit = connectionLevel(metrics.totalConnections, metrics.maxConnections) !== "ok";

    return [
      {
        label: t("Conexiones"),
        icon: "plug" as const,
        value: `${metrics.totalConnections} / ${metrics.maxConnections}`,
        series: monitor.history.map((sample) => sample.connections) as (number | null)[],
        tone: nearLimit
          ? connectionLevel(metrics.totalConnections, metrics.maxConnections) === "bad"
            ? "bad"
            : "warn"
          : null,
        hint: nearLimit ? t("cerca del máximo configurado") : null,
      },
      {
        label: t("Activas"),
        icon: "play" as const,
        value: String(metrics.activeConnections),
        series: monitor.history.map((sample) => sample.active) as (number | null)[],
        tone: null,
        hint: null,
      },
      {
        label: t("Inactivas en transacción"),
        icon: "pin" as const,
        value: String(metrics.idleInTransaction),
        tone: metrics.idleInTransaction > 0 ? "warn" : null,
        hint: metrics.idleInTransaction > 0 ? t("retienen candados sin trabajar") : null,
      },
      {
        label: t("Esperando"),
        icon: "lock" as const,
        value: String(metrics.waitingConnections),
        tone: metrics.waitingConnections > 0 ? "bad" : null,
        hint: metrics.waitingConnections > 0 ? t("bloqueadas por otra sesión") : null,
      },
      {
        label: t("Transacciones/s"),
        icon: "gauge" as const,
        value: decimal(metrics.transactionsPerSecond),
        series: monitor.history.map((sample) => sample.transactionsPerSecond),
        tone: null,
        hint: null,
      },
      {
        label: t("Transacción más vieja"),
        icon: "clock" as const,
        value: duration(metrics.longestTransactionSeconds),
        tone: null,
        hint: null,
      },
    ];
  });

  /**
   * Los gráficos de actividad en vivo empiezan plegados: las lecturas de arriba ya dicen el valor
   * actual con su tendencia, y los gráficos desplazaban la lista de sesiones fuera de la primera
   * pantalla. Se recuerda lo que se elija.
   */
  const CHARTS_KEY = "pgforge.monitor.charts";
  let showCharts = $state(
    (() => {
      try {
        return localStorage.getItem(CHARTS_KEY) === "1";
      } catch {
        return false;
      }
    })(),
  );

  function toggleCharts() {
    showCharts = !showCharts;
    try {
      localStorage.setItem(CHARTS_KEY, showCharts ? "1" : "0");
    } catch {
      // Sin `localStorage` no se recuerda esta vez.
    }
  }

  /** El panel de alertas empieza plegado: es configuración, no lo primero que se mira. */
  let showAlerts = $state(false);

  const health = $derived(metrics ? assessHealth(metrics) : { level: "ok" as const, issues: [] });

  const HEALTH_TITLE = $derived({
    ok: t("Todo en orden"),
    warn: t("Para revisar"),
    bad: t("Requiere atención"),
  } as const);
  const HEALTH_BOX = {
    ok: "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
    warn: "border-amber-500/40 bg-amber-500/10 text-amber-800 dark:text-amber-300",
    bad: "border-rose-500/40 bg-rose-500/10 text-rose-700 dark:text-rose-300",
  } as const;

  const TILE_TONE: Record<string, string> = {
    bad: "text-rose-600 dark:text-rose-400",
    warn: "text-amber-600 dark:text-amber-400",
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

  /**
   * Las estadísticas de tablas e índices no llegan por el monitor (que muestrea cada pocos segundos):
   * se leen al abrir la pestaña, al cambiar de base y cuando se aprieta «Actualizar». Cada lectura
   * lleva un número de corrida: si se pide otra antes de que vuelva la anterior —cambiar de pestaña
   * o de base rápido—, la respuesta vieja se descarta en vez de pisar a la nueva.
   */
  let statsRun = 0;
  let statsDb: string | null = null;
  let statsLoading = $state(false);
  let statsLoadedAt = $state<number | null>(null);
  let now = $state(Date.now());
  let tablesError = $state<string | null>(null);
  let indexesError = $state<string | null>(null);

  // Solo late mientras se mira una pestaña de estadísticas: es el «Actualizado hace …».
  $effect(() => {
    if (tab === "sesiones") return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  /** Tras releer, la selección apunta a la fila nueva —con los números de ahora— o se suelta. */
  function reselect<T>(selected: T | null, rows: T[], key: (row: T) => string): T | null {
    if (!selected) return null;
    const id = key(selected);
    return rows.find((row) => key(row) === id) ?? null;
  }

  async function loadStats(current: Tab) {
    if (current === "sesiones" || OWN_TABS.includes(current)) return;
    const run = ++statsRun;
    const stale = () => run !== statsRun;

    // Lo leído de otra base no vale para esta: mejor una lista vacía que tablas ajenas.
    if (statsDb !== monitor.database) {
      statsDb = monitor.database;
      tables = [];
      indexes = [];
      redundant = [];
      bloat = [];
      statements = [];
      selectedTable = selectedIndex = selectedRedundant = selectedBloat = selectedStatement = null;
      statsLoadedAt = null;
    }

    statsLoading = true;
    dropError = null;
    try {
      if (current === "tablas") {
        tablesError = null;
        const result = await tableStats(profileId);
        if (stale()) return;
        tables = result;
        selectedTable = reselect(selectedTable, result, (t) => `${t.schema}.${t.table}`);
      } else if (current === "indices") {
        indexesError = null;
        // Si no se puede leer, la grilla igual sirve: solo falta el «desde cuándo».
        const sincePromise = statsWindow(profileId).catch(() => null);
        const result = await indexStats(profileId);
        const since = await sincePromise;
        if (stale()) return;
        indexes = result;
        usageWindow = since;
        selectedIndex = reselect(selectedIndex, result, (i) => `${i.schema}.${i.index}`);
      } else if (current === "duplicados") {
        redundantError = null;
        const result = await redundantIndexes(profileId);
        if (stale()) return;
        redundant = result;
        selectedRedundant = reselect(selectedRedundant, result, keyOf);
      } else if (current === "bloat") {
        bloatError = null;
        const available = await hasBloatStats(profileId);
        const result = available ? await tableBloat(profileId) : [];
        if (stale()) return;
        bloatAvailable = available;
        bloat = result;
        selectedBloat = reselect(selectedBloat, result, (t) => `${t.schema}.${t.table}`);
      } else if (current === "consultas") {
        statementsError = null;
        const available = await hasStatementStats(profileId);
        const result = available ? await statementStats(profileId) : [];
        if (stale()) return;
        statementsAvailable = available;
        statements = result;
      }
      statsLoadedAt = Date.now();
      now = statsLoadedAt;
    } catch (error) {
      if (stale()) return;
      const message = describeError(error);
      if (current === "tablas") tablesError = message;
      else if (current === "indices") indexesError = message;
      else if (current === "duplicados") redundantError = message;
      else if (current === "bloat") bloatError = message;
      else statementsError = message;
    } finally {
      if (!stale()) statsLoading = false;
    }
  }

  $effect(() => {
    const current = tab;
    // Se recargan al cambiar de pestaña y cuando el monitor queda listo en otra base: leer
    // monitor.profileId/database hace que el efecto reaccione a la reapertura de la sesión (que pasa
    // por null), y saltea el hueco entre parar y arrancar en el que el monitor no está activo.
    void monitor.database;
    if (!monitor.profileId) return;
    // Sin untrack, loadStats leería las listas y el efecto se reiniciaría con cada respuesta.
    untrack(() => loadStats(current));
  });

  let sessionsPanel = $state<ReturnType<typeof SessionsPanel> | null>(null);

  async function act(pids: number[], kind: "cancel" | "terminate") {
    confirming = null;
    // De a una y en orden: el servidor decide por cada PID si todavía existe, y un fallo a la mitad
    // no tiene que dejar sin contar lo que sí se hizo.
    let done = 0;
    let missing = 0;
    let failure: string | null = null;
    for (const pid of pids) {
      try {
        const ok =
          kind === "cancel"
            ? await cancelBackend(profileId, pid)
            : await terminateBackend(profileId, pid);
        if (ok) done += 1;
        else missing += 1;
      } catch (error) {
        failure = describeError(error);
        break;
      }
    }
    // La lista congelada sigue mostrando la sesión recién terminada, que es justo lo que ya no
    // está: se suelta la elegida y, pasado el tiempo de la próxima muestra, se toma otra foto.
    if (done > 0) {
      if (kind === "terminate" && selectedPid !== null && pids.includes(selectedPid)) {
        selectedPid = null;
      }
      setTimeout(() => sessionsPanel?.refresh(), 2500);
    }
    const missingText = missing > 0 ? t("; {count} ya no existían", { count: missing }) : "";
    actionFailed = failure !== null || (done === 0 && missing > 0);
    actionMessage =
      failure ??
      (pids.length === 1
        ? done === 1
          ? kind === "cancel"
            ? t("Se pidió cancelar la consulta del PID {pid}.", { pid: pids[0] })
            : t("Se terminó la sesión {pid}.", { pid: pids[0] })
          : t("El PID {pid} ya no existe.", { pid: pids[0] })
        : kind === "cancel"
          ? t("Se pidió cancelar la consulta de {done} de {total} sesiones{missing}.", { done, total: pids.length, missing: missingText })
          : t("Se pidió terminar {done} de {total} sesiones{missing}.", { done, total: pids.length, missing: missingText }));
  }

  const tableColumns = $derived<Column<TableStat>[]>([
    { key: "schema", header: t("Esquema"), width: 130, value: (t) => t.schema },
    { key: "table", header: t("Tabla"), width: 200, value: (t) => t.table },
    {
      key: "live",
      header: t("Filas vivas"),
      width: 110,
      align: "right",
      value: (t) => count(t.liveTuples),
      sort: (t) => t.liveTuples ?? -1,
    },
    {
      key: "dead",
      header: t("Muertas"),
      width: 100,
      align: "right",
      value: (t) => count(t.deadTuples),
      sort: (t) => t.deadTuples ?? -1,
    },
    {
      key: "ratio",
      header: t("% muertas (est.)"),
      width: 120,
      align: "right",
      value: (t) => percent(t.deadRatio),
      sort: (t) => t.deadRatio ?? -1,
      tone: (t) => ((t.deadRatio ?? 0) > 0.2 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
    {
      key: "total",
      header: t("Tamaño"),
      width: 100,
      align: "right",
      value: (t) => bytes(t.totalBytes),
      sort: (t) => t.totalBytes ?? -1,
    },
    {
      key: "idx",
      header: t("Índices"),
      width: 100,
      align: "right",
      value: (t) => bytes(t.indexBytes),
      sort: (t) => t.indexBytes ?? -1,
    },
    {
      key: "seq",
      header: t("Seq scans"),
      width: 100,
      align: "right",
      value: (t) => count(t.sequentialScans),
      sort: (t) => t.sequentialScans ?? -1,
    },
    {
      key: "iscan",
      header: t("Idx scans"),
      width: 100,
      align: "right",
      value: (t) => count(t.indexScans),
      sort: (t) => t.indexScans ?? -1,
    },
    {
      key: "vac",
      header: t("Último autovacuum"),
      width: 160,
      value: (t) => ago(t.lastAutovacuumSeconds),
      sort: (t) => t.lastAutovacuumSeconds ?? Number.MAX_SAFE_INTEGER,
    },
  ]);

  const redundantColumns = $derived<Column<Redundancy>[]>([
    { key: "schema", header: t("Esquema"), width: 120, value: (r) => r.schema },
    { key: "table", header: t("Tabla"), width: 180, value: (r) => r.table },
    { key: "index", header: t("Índice que sobra"), width: 240, value: (r) => r.index },
    { key: "coveredBy", header: t("Ya lo cubre"), width: 240, value: (r) => r.coveredBy },
    {
      key: "kind",
      header: t("Por qué"),
      width: 150,
      value: (r) => (r.kind === "duplicate" ? t("mismas columnas") : t("es el principio del otro")),
    },
    {
      key: "scans",
      header: t("Usos"),
      width: 90,
      align: "right",
      value: (r) => count(r.scans),
      sort: (r) => r.scans,
    },
    {
      key: "size",
      header: t("Tamaño"),
      width: 100,
      align: "right",
      value: (r) => bytes(r.bytes),
      sort: (r) => r.bytes,
    },
  ]);

  /**
   * Borra el índice elegido. Va con CONCURRENTLY salvo que sea un índice particionado, que el
   * servidor rechaza así: lo dice `dropSql`, que es la sentencia que se mostró en la confirmación.
   */
  async function dropRedundant(target: Redundancy) {
    droppingIndex = null;
    if (!(await confirmMutation(profileId, t("Se va a borrar un índice del servidor."), target.dropSql)))
      return;
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

  /**
   * Borra un índice de la lista de estadísticas. Siempre con CONCURRENTLY: esa lista no trae índices
   * particionados, y así el borrado no frena las escrituras sobre la tabla. Lo protegido ni llega
   * acá —el botón se apaga con el motivo—, y lo que el servidor rechace igual cae en `dropError`.
   */
  async function dropStat(target: IndexStat) {
    droppingStat = null;
    if (
      !(await confirmMutation(
        profileId,
        t("Se va a borrar un índice del servidor."),
        target.dropSql ?? `DROP INDEX ${target.schema}.${target.index}`,
      ))
    )
      return;
    try {
      await indexDrop(profileId, target.schema, target.index, false, true, database ?? undefined);
      const key = `${target.schema}.${target.index}`;
      indexes = indexes.filter((item) => `${item.schema}.${item.index}` !== key);
      if (selectedIndex && `${selectedIndex.schema}.${selectedIndex.index}` === key) {
        selectedIndex = null;
      }
      dropError = null;
    } catch (error) {
      dropError = describeError(error);
    }
  }

  /**
   * «Último uso» solo existe desde PG 16. Antes la columna saldría entera en «nunca», que se lee
   * como un dato y es una ausencia: mejor que no esté.
   */
  const lastScanColumn = $derived<Column<IndexStat>>({
    key: "lastScan",
    header: t("Último uso"),
    width: 120,
    align: "right",
    value: (i) => ago(i.lastScanSeconds),
    // Lo que nunca se usó va al fondo al ordenar de más reciente a más viejo.
    sort: (i) => i.lastScanSeconds ?? Number.MAX_VALUE,
  });

  const indexColumns: Column<IndexStat>[] = $derived([
    { key: "schema", header: t("Esquema"), width: 130, value: (i) => i.schema },
    { key: "table", header: t("Tabla"), width: 200, value: (i) => i.table },
    { key: "index", header: t("Índice"), width: 260, value: (i) => i.index },
    {
      key: "scans",
      header: t("Usos"),
      width: 100,
      align: "right",
      value: (i) => count(i.scans),
      sort: (i) => i.scans ?? -1,
    },
    ...(usageWindow?.tracksLastScan ? [lastScanColumn] : []),
    {
      key: "size",
      header: t("Tamaño"),
      width: 100,
      align: "right",
      value: (i) => bytes(i.bytes),
      sort: (i) => i.bytes ?? -1,
    },
    {
      key: "kind",
      header: t("Tipo"),
      width: 120,
      value: (i) => (i.isPrimary ? t("primaria") : i.isUnique ? t("única") : t("secundario")),
    },
    {
      key: "state",
      header: t("Observación"),
      width: 200,
      value: (i) =>
        // «Sin usos» y no «nunca se usó»: cuenta desde el último reinicio de las estadísticas.
        !i.isValid ? t("INVÁLIDO: hay que reconstruirlo") : i.unused ? t("sin usos registrados") : "",
      tone: (i) =>
        !i.isValid
          ? "text-rose-600 dark:text-rose-400"
          : i.unused
            ? "text-amber-600 dark:text-amber-400"
            : undefined,
    },
  ]);

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
      const tab = await openQuery(profileId, target ?? database ?? "", t("Consulta"));
      tab.sql = sql;
    } catch (error) {
      actionMessage = describeError(error);
    }
  }

  /**
   * Explica una sentencia de la lista. Sin parámetros se explica en el acto, en una pestaña nueva;
   * con `$1…` se abre escrita y con un aviso (ver `statement-explain.ts`): el plan depende de los
   * valores y el servidor no puede inventarlos.
   */
  async function explainStatement(statement: StatementStat) {
    if (!statement.query) return;
    const target = statement.database ?? database ?? "";
    try {
      if (needsValues(statement.query)) {
        await openInQuery(withValuesHint(statement.query), target);
        return;
      }
      const tab = await openQuery(profileId, target, t("Plan"));
      tab.sql = statement.query;
      await tab.explain(statement.query, 0, { analyze: false, buffers: false, verbose: true });
    } catch (error) {
      actionMessage = describeError(error);
    }
  }

  const statementColumns = $derived<Column<StatementStat>[]>([
    { key: "database", header: t("Base"), width: 120, value: (s) => s.database ?? "—" },
    { key: "user", header: t("Usuario"), width: 110, value: (s) => s.user ?? "—" },
    {
      key: "query",
      header: t("Sentencia"),
      width: 620,
      value: (s) => oneLine(s.query, 400),
      title: (s) => s.query ?? undefined,
    },
    {
      key: "calls",
      header: t("Llamadas"),
      width: 110,
      align: "right",
      value: (s) => count(s.calls),
      sort: (s) => s.calls,
    },
    {
      key: "total",
      header: t("Total"),
      width: 110,
      align: "right",
      value: (s) => duration(s.totalMs / 1000),
      sort: (s) => s.totalMs,
    },
    {
      key: "mean",
      header: t("Media"),
      width: 110,
      align: "right",
      value: (s) => duration(s.meanMs / 1000),
      sort: (s) => s.meanMs,
    },
    {
      key: "rows",
      header: t("Filas"),
      width: 110,
      align: "right",
      value: (s) => count(s.rows),
      sort: (s) => s.rows,
    },
  ]);

  const TABS = $derived<{ value: Tab; label: string; icon: IconName }[]>([
    { value: "sesiones", label: t("Sesiones"), icon: "gauge" },
    { value: "tablas", label: t("Tablas"), icon: "table" },
    { value: "indices", label: t("Índices"), icon: "index" },
    { value: "duplicados", label: t("Índices de más"), icon: "compare" },
    { value: "bloat", label: t("Bloat"), icon: "warn" },
    { value: "consultas", label: t("Consultas lentas"), icon: "sql" },
    { value: "prevencion", label: t("Prevención"), icon: "lock" },
    { value: "espacio", label: t("Espacio"), icon: "database" },
    { value: "cache", label: t("Caché"), icon: "chart" },
    { value: "conexiones", label: t("Conexiones"), icon: "plug" },
    { value: "servidor", label: t("Servidor"), icon: "sliders" },
  ]);

  const bloatColumns = $derived<Column<TableBloat>[]>([
    { key: "schema", header: t("Esquema"), width: 130, value: (t) => t.schema },
    { key: "table", header: t("Tabla"), width: 200, value: (t) => t.table },
    {
      key: "total",
      header: t("Tamaño"),
      width: 110,
      align: "right",
      value: (t) => bytes(t.totalBytes),
      sort: (t) => t.totalBytes,
    },
    {
      key: "free",
      header: t("Libre (est.)"),
      width: 110,
      align: "right",
      value: (t) => bytes(t.freeBytes),
      sort: (t) => t.freeBytes,
    },
    {
      key: "freeRatio",
      header: t("% libre"),
      width: 100,
      align: "right",
      value: (t) => percent(t.freeRatio),
      sort: (t) => t.freeRatio,
      tone: (t) => (t.freeRatio > 0.3 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
    {
      key: "deadRatio",
      header: t("% muertas"),
      width: 100,
      align: "right",
      value: (t) => percent(t.deadRatio),
      sort: (t) => t.deadRatio,
      tone: (t) => (t.deadRatio > 0.2 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
  ]);

  /** Cuántas sesiones esperan a otra: el número que decide si hay que mirar la pestaña. */
  const blocked = $derived(backends.filter((backend) => backend.blockedBy.length > 0).length);
</script>

{#snippet refreshControl()}
  <span class="text-[11px] muted" aria-live="polite">
    {#if statsLoading}
      {t("Actualizando…")}
    {:else if statsLoadedAt}
      {@const seconds = Math.max(0, Math.round((now - statsLoadedAt) / 1000))}
      {seconds < 5 ? t("Actualizado ahora") : t("Actualizado {since}", { since: ago(seconds) })}
    {/if}
  </span>
  <button
    class="btn btn-sm"
    disabled={statsLoading}
    title={t("Volver a leer estas estadísticas del servidor: no se actualizan solas")}
    onclick={() => loadStats(tab)}
  >
    {#if statsLoading}
      <span class="spinner"></span>
    {:else}
      <Icon name="refresh" size={12} />
    {/if}
    {t("Actualizar")}
  </button>
{/snippet}

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
          {#if item.value === "sesiones" && blocked > 0}
            <span class="tag tag-bad px-1 py-0 text-[10px]">{blocked}</span>
          {/if}
        </button>
      {/each}
    </div>

    {#if databases.length > 1 || database}
      <span class="toolbar-sep"></span>
    {/if}

    {#if databases.length > 1}
      <label class="check" title={t("Base cuyas tablas, índices y sentencias se muestran")}>
        {t("Base")}
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
        title={t("VACUUM, ANALYZE o REINDEX sobre toda la base {name}", { name: database })}
        onclick={() => (maintenanceTarget = { kind: "database", name: database! })}
      >
        <Icon name="gauge" size={12} />
        {t("Mantenimiento de la base")}
      </button>
    {/if}

    <span class="ml-auto"></span>

    <button
      class="btn btn-sm"
      title={t("Guarda un informe de salud del servidor para pasarlo al equipo o adjuntarlo a un ticket")}
      onclick={() => (reporting = true)}
    >
      <Icon name="download" size={12} />
      {t("Informe")}
    </button>

    <!-- Filtros e intervalo son de la muestra en vivo: en las pestañas de estadísticas no hacen nada,
         y el ícono de refrescar junto al intervalo se leía como «actualizar» estas listas. -->
    {#if tab === "sesiones"}
    <label class="check">
      <input
        type="checkbox"
        checked={monitor.filter.includeIdle}
        onchange={(event) =>
          monitor.setFilter({ ...monitor.filter, includeIdle: event.currentTarget.checked })}
      />
      {t("Inactivas")}
    </label>

    <label class="check">
      <input
        type="checkbox"
        checked={monitor.filter.includeBackground}
        onchange={(event) =>
          monitor.setFilter({ ...monitor.filter, includeBackground: event.currentTarget.checked })}
      />
      {t("Procesos internos")}
    </label>
    {/if}

    <span class="toolbar-sep"></span>

    <button
      class="btn btn-sm"
      aria-pressed={showAlerts}
      title={t("Reglas de alerta con aviso del sistema; solo corren mientras este monitoreo está abierto")}
      onclick={() => (showAlerts = !showAlerts)}
    >
      <Icon name="warn" size={12} />
      {t("Alertas")}
    </button>

    {#if tab === "sesiones"}
    <label class="check">
      <Icon name="clock" size={11} />
      <select
        class="field py-0.5 text-xs"
        title={t("Cada cuánto se toma una muestra de las sesiones y las métricas")}
        value={monitor.intervalMs}
        onchange={(event) => monitor.setInterval(Number(event.currentTarget.value))}
      >
        <option value={1000}>1 s</option>
        <option value={2000}>2 s</option>
        <option value={5000}>5 s</option>
        <option value={15000}>15 s</option>
      </select>
    </label>
    {/if}
  </div>

  {#if monitor.error}
    <Alert tone="bad">{monitor.error}</Alert>
  {/if}

  <div class="bg-zinc-50/70 dark:bg-black/15">
    {#if metrics}
      <!-- El veredicto antes que las fichas: para saber si todo está bien no hay que leer las seis. -->
      <div
        class="mx-4 mt-4 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border px-3 py-2 text-sm
               {HEALTH_BOX[health.level]}"
        role="status"
      >
        <Icon name={health.level === "ok" ? "check" : "warn"} size={14} />
        <span class="font-semibold">{HEALTH_TITLE[health.level]}</span>
        {#each health.issues as issue, index (index)}
          <span class="text-xs">{index === 0 ? "" : "· "}{issue.text}</span>
        {/each}
        <button
          class="btn btn-sm btn-ghost ml-auto"
          aria-pressed={showCharts}
          title={t("Muestra u oculta los gráficos de actividad en vivo")}
          onclick={toggleCharts}
        >
          <Icon name="chart" size={12} />
          {t("Gráficos")}
        </button>
      </div>
      {#if showAlerts}
        <AlertsPanel {profileId} />
      {/if}
      <!-- Una sola tira de seis lecturas, con tendencia donde hay muestras. Las fichas de antes
           ocupaban casi 250 px de alto antes de llegar a la primera sesión. El motivo de un color
           viaja en el `title`: el veredicto de arriba ya lo dice con palabras. -->
      <div class="mx-4 mt-3 mb-3 flex flex-wrap overflow-hidden rounded-md border border-zinc-200 bg-white dark:border-zinc-700 dark:bg-zinc-800">
        {#each tiles as tile (tile.label)}
          <div
            class="flex min-w-32 flex-1 basis-32 flex-col gap-0.5 border-r border-zinc-200 px-3 py-1.5 last:border-r-0 dark:border-zinc-700"
            title={tile.hint ?? tile.label}
          >
            <span class="truncate text-[11px] muted">{tile.label}</span>
            <span
              class="font-mono text-lg leading-tight font-semibold tabular-nums {tile.tone
                ? TILE_TONE[tile.tone]
                : ''}"
            >
              {tile.value}
            </span>
            {#if tile.series}
              <Spark values={tile.series} width={96} height={14} />
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <Skeleton rows={6} label={t("Tomando la primera muestra…")} />
    {/if}

    {#if tab === "sesiones" && showCharts}
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
              {t("Actividad en vivo")}
            </span>
            <span class="text-[11px] muted">{t("muestra cada {seconds}s", { seconds: monitor.intervalMs / 1000 })}</span>
          </div>
          <div class="grid grid-cols-2 divide-x divide-zinc-200 xl:grid-cols-4 dark:divide-zinc-700">
            <Chart label={t("Conexiones")} data={connectionsSeries} />
            <Chart label={t("Activas")} data={activeSeries} color="#f59e0b" />
            <Chart
              label={t("Transacciones/s")}
              data={tpsSeries}
              color="#10b981"
              formatValue={oneDecimal}
            />
            <Chart
              label={t("Aciertos de caché")}
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
    {#if actionMessage}
      <Alert tone={actionFailed ? "bad" : "ok"} onclose={() => (actionMessage = null)}>
        {actionMessage}
      </Alert>
    {/if}

    <SessionsPanel
      bind:this={sessionsPanel}
      {backends}
      blocking={snapshot?.blocking ?? []}
      bind:selectedPid
      {locks}
      oncancel={(pids) => (confirming = { pids, kind: "cancel" })}
      onterminate={(pids) => (confirming = { pids, kind: "terminate" })}
      onopen={(sql, db) => openInQuery(sql, db)}
    />
  {:else if tab === "tablas"}
    <div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
      <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
        <div class="card-head justify-between">
          <span class="card-title flex items-center gap-1.5">
            <Icon name="table" size={13} />
            {t("Estadísticas de tablas")}
          </span>
          <div class="flex items-center gap-2">
            {@render refreshControl()}
            <span class="tag tag-neutral font-mono">{count(tables.length)}</span>
          </div>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            {t("La proporción de tuplas muertas es una estimación sobre los contadores de estadísticas, no una medición del espacio desperdiciado.")}
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedTable}
            title={selectedTable
              ? t("VACUUM, ANALYZE o REINDEX sobre {name}", { name: `${selectedTable.schema}.${selectedTable.table}` })
              : t("Elegí una tabla de la lista")}
            onclick={() =>
              selectedTable &&
              (maintenanceTarget = {
                kind: "table",
                schema: selectedTable.schema,
                name: selectedTable.table,
              })}
          >
            <Icon name="gauge" size={12} />
            {t("Mantenimiento")}
          </button>
        </div>
        {#if tablesError}
          <Alert tone="bad">{tablesError}</Alert>
        {/if}
        <div class="min-h-0 flex-1">
          <DataGrid
            columns={tableColumns}
            rows={tables}
            rowKey={(table) => `${table.schema}.${table.table}`}
            selectedKey={selectedTable ? `${selectedTable.schema}.${selectedTable.table}` : null}
            onselect={(table) => (selectedTable = table)}
            sortable
            empty={t("No hay estadísticas de tablas en esta base.")}
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
            {t("Estadísticas de índices")}
          </span>
          <div class="flex items-center gap-2">
            {@render refreshControl()}
            <span class="tag tag-neutral font-mono">{count(indexes.length)}</span>
          </div>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            {t("Un índice que quedó inválido se reconstruye con REINDEX.")}
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedIndex}
            title={selectedIndex
              ? t("REINDEX sobre {name}", { name: `${selectedIndex.schema}.${selectedIndex.index}` })
              : t("Elegí un índice de la lista")}
            onclick={() =>
              selectedIndex &&
              (maintenanceTarget = {
                kind: "index",
                schema: selectedIndex.schema,
                name: selectedIndex.index,
              })}
          >
            <Icon name="gauge" size={12} />
            {t("Mantenimiento")}
          </button>
          <button
            class="btn btn-sm btn-danger"
            disabled={!selectedIndex?.dropSql}
            title={!selectedIndex
              ? t("Elegí un índice de la lista")
              : selectedIndex.protectedBy
                ? t("No se puede borrar: {reason}", { reason: selectedIndex.protectedBy })
                : selectedIndex.dropSql}
            onclick={() => (droppingStat = selectedIndex)}
          >
            <Icon name="trash" size={12} />
            {t("Borrar el índice…")}
          </button>
        </div>
        {#if countingSince}
          <div class="divider-b flex items-center gap-1.5 px-3 py-1 text-xs muted">
            <Icon name="info" size={11} class="shrink-0" />
            {countingSince}
          </div>
        {/if}
        {#if indexesError}
          <Alert tone="bad">{indexesError}</Alert>
        {/if}
        {#if dropError}
          <Alert tone="bad">{dropError}</Alert>
        {/if}
        <div class="min-h-0 flex-1">
          <DataGrid
            columns={indexColumns}
            rows={indexes}
            rowKey={(index) => `${index.schema}.${index.index}`}
            selectedKey={selectedIndex ? `${selectedIndex.schema}.${selectedIndex.index}` : null}
            onselect={(index) => (selectedIndex = index)}
            sortable
            empty={t("No hay estadísticas de índices en esta base.")}
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
            {t("Índices que sobran")}
          </span>
          <div class="flex items-center gap-2">
            {@render refreshControl()}
            <span class="tag tag-neutral font-mono">{count(redundant.length)}</span>
          </div>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            {t("Un índice que otro ya cubre ocupa disco y hace más lenta cada escritura sin acelerar ninguna lectura. Lo que sostiene algo —una restricción, una clave foránea, la identidad de réplica, el orden de un CLUSTER— nunca aparece acá.")}
          </span>
          <button
            class="btn btn-sm btn-danger ml-auto"
            disabled={!selectedRedundant}
            title={selectedRedundant
              ? t("DROP INDEX sobre {name}", { name: `${selectedRedundant.schema}.${selectedRedundant.index}` })
              : t("Elegí un índice de la lista")}
            onclick={() => (droppingIndex = selectedRedundant)}
          >
            <Icon name="trash" size={12} />
            {t("Borrar el índice…")}
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
              empty={t("Ningún índice de esta base está cubierto por otro.")}
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
            {t("Bloat estimado")}
          </span>
          <div class="flex items-center gap-2">
            {@render refreshControl()}
            <span class="tag tag-neutral font-mono">{count(bloat.length)}</span>
          </div>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            {t("Medición aproximada con pgstattuple: el espacio libre se recupera con VACUUM FULL, que bloquea la tabla mientras corre.")}
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedBloat}
            title={selectedBloat
              ? t("VACUUM sobre {name}", { name: `${selectedBloat.schema}.${selectedBloat.table}` })
              : t("Elegí una tabla de la lista")}
            onclick={() =>
              selectedBloat &&
              (maintenanceTarget = {
                kind: "table",
                schema: selectedBloat.schema,
                name: selectedBloat.table,
              })}
          >
            <Icon name="gauge" size={12} />
            {t("Mantenimiento")}
          </button>
        </div>
        <div class="min-h-0 flex-1">
          {#if bloatError}
            <Alert tone="bad">{bloatError}</Alert>
          {:else if bloatAvailable === false}
            <Empty
              icon="info"
              title={t("Falta la extensión pgstattuple")}
              hint={t("Ejecutá CREATE EXTENSION pgstattuple; en esta base para estimar el bloat de las tablas.")}
            />
          {:else}
            <DataGrid
              columns={bloatColumns}
              rows={bloat}
              rowKey={(table) => `${table.schema}.${table.table}`}
              selectedKey={selectedBloat ? `${selectedBloat.schema}.${selectedBloat.table}` : null}
              onselect={(table) => (selectedBloat = table)}
              sortable
              empty={t("No hay tablas con bloat que estimar en esta base.")}
            />
          {/if}
        </div>
      </div>
    </div>
  {:else if tab === "prevencion"}
    {#key monitor.database}<PreventPanel {profileId} database={monitor.database} />{/key}
  {:else if tab === "espacio"}
    {#key monitor.database}<SpacePanel {profileId} database={monitor.database} />{/key}
  {:else if tab === "cache"}
    {#key monitor.database}<CachePanel {profileId} database={monitor.database} />{/key}
  {:else if tab === "conexiones"}
    {#key monitor.database}<ConnectionsPanel {profileId} database={monitor.database} />{/key}
  {:else if tab === "servidor"}
    {#key monitor.database}<ServerPanel {profileId} database={monitor.database} />{/key}
  {:else}
    <div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
      <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
        <div class="card-head justify-between">
          <span class="card-title flex items-center gap-1.5">
            <Icon name="sql" size={13} />
            {t("Consultas más lentas")}
          </span>
          <div class="flex items-center gap-2">
            {@render refreshControl()}
            <span class="tag tag-neutral font-mono">{count(statements.length)}</span>
          </div>
        </div>
        <div class="toolbar divider-b">
          <span class="text-xs muted">
            {t("El texto viene normalizado: los valores aparecen como $1, $2. Al abrirlo en una consulta hay que completarlos antes de explicar.")}
          </span>
          <button
            class="btn btn-sm ml-auto"
            disabled={!selectedStatement?.query}
            title={selectedStatement?.query
              ? needsValues(selectedStatement.query)
                ? t("Abre la sentencia para que le pongas valores a los parámetros y la expliques")
                : t("Muestra el plan estimado de esta sentencia, sin ejecutarla")
              : t("Elegí una consulta de la lista")}
            onclick={() => selectedStatement && explainStatement(selectedStatement)}
          >
            <Icon name="plan" size={12} />
            {t("Explicar")}
          </button>
          <button
            class="btn btn-sm"
            disabled={!selectedStatement?.query}
            title={selectedStatement?.query
              ? t("Abre una pestaña de consulta con este texto")
              : t("Elegí una consulta de la lista")}
            onclick={() =>
              selectedStatement?.query &&
              openInQuery(selectedStatement.query, selectedStatement.database)}
          >
            <Icon name="play" size={12} />
            {t("Abrir en una consulta")}
          </button>
        </div>
        <div class="min-h-0 flex-1">
          {#if statementsError}
            <Alert tone="bad">{statementsError}</Alert>
          {:else if statementsAvailable === false}
            <Empty
              icon="info"
              title={t("Falta la extensión pg_stat_statements")}
              hint={t("Agregala a shared_preload_libraries, reiniciá el servidor y ejecutá CREATE EXTENSION pg_stat_statements; en esta base.")}
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
              empty={t("Todavía no hay consultas registradas.")}
            />
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

{#if confirming}
  {@const many = confirming.pids.length > 1}
  {@const cancelTitle = many ? t("Cancelar las consultas") : t("Cancelar la consulta")}
  {@const terminateTitle = many ? t("Terminar las sesiones") : t("Terminar la sesión")}
  <Confirm
    title={confirming.kind === "cancel" ? cancelTitle : terminateTitle}
    message={confirming.kind === "cancel"
      ? many
        ? t("Se le pide al servidor que aborte la consulta de {count} sesiones (PID {pids}). Las sesiones siguen conectadas y sus transacciones quedan abiertas pero abortadas.", { count: confirming.pids.length, pids: confirming.pids.join(", ") })
        : t("Se le pide al servidor que aborte la consulta del PID {pid}. La sesión sigue conectada y su transacción queda abierta pero abortada.", { pid: confirming.pids[0] })
      : many
        ? t("Se cierran {count} sesiones por completo (PID {pids}): sus transacciones se revierten y los clientes pierden la conexión sin aviso.", { count: confirming.pids.length, pids: confirming.pids.join(", ") })
        : t("Se cierra la sesión {pid} por completo: su transacción se revierte y el cliente pierde la conexión sin aviso.", { pid: confirming.pids[0] })}
    confirmLabel={confirming.kind === "cancel" ? cancelTitle : terminateTitle}
    onconfirm={() => confirming && act(confirming.pids, confirming.kind)}
    onclose={() => (confirming = null)}
  />
{/if}
{#if droppingIndex}
  <Confirm
    title={t("Borrar «{name}»", { name: droppingIndex.index })}
    message={t("{covered} cubre lo mismo, así que las consultas que hoy usan este índice pueden seguir usando aquel. Se ejecuta: {sql}", { covered: droppingIndex.coveredBy, sql: droppingIndex.dropSql })}
    confirmLabel={t("Borrar el índice")}
    danger
    onconfirm={() => droppingIndex && dropRedundant(droppingIndex)}
    onclose={() => (droppingIndex = null)}
  />
{/if}

{#if droppingStat}
  <Confirm
    title={t("Borrar «{name}»", { name: droppingStat.index })}
    message={droppingStat.scans === 0
      ? t("No registra usos desde el último reinicio de las estadísticas. Se ejecuta: {sql}", { sql: droppingStat.dropSql ?? "" })
      : t("Registra {count} usos: las consultas que hoy lo usan van a tener que resolverse de otra forma. Se ejecuta: {sql}", { count: count(droppingStat.scans), sql: droppingStat.dropSql ?? "" })}
    confirmLabel={t("Borrar el índice")}
    danger
    onconfirm={() => droppingStat && dropStat(droppingStat)}
    onclose={() => (droppingStat = null)}
  />
{/if}

{#if reporting}
  <ReportDialog
    {profileId}
    server={explorer.profiles.find((p) => p.id === profileId)?.name ?? t("servidor")}
    database={monitor.database ?? database ?? ""}
    metrics={monitor.snapshot?.metrics ?? null}
    onclose={() => (reporting = false)}
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
