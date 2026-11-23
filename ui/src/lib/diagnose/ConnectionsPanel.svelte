<script lang="ts">
  import { untrack } from "svelte";
  import DataGrid, { type Column } from "../DataGrid.svelte";
  import { connectionLevel } from "../health";
  import { t } from "../i18n.svelte";
  import { count, duration, percent } from "../format";
  import { connectionReport, type ConnectionGroup, type ConnectionReport } from "../ipc";
  import { Loader } from "./loader.svelte";
  import PanelFrame from "./PanelFrame.svelte";

  /**
   * Quién tiene las conexiones. El total ya está en la ficha del monitoreo; esto responde la
   * pregunta siguiente: ¿de qué aplicación, con qué rol, desde dónde, y cuántas están sin hacer
   * nada? Es lo que decide si hace falta un pooler o solo cerrar una aplicación que las deja
   * abiertas.
   */
  let { profileId, database }: { profileId: string; database: string | null } = $props();

  const report = new Loader<ConnectionReport>();

  function refresh() {
    void report.load(() => connectionReport(profileId));
  }

  $effect(() => {
    void database;
    void profileId;
    untrack(() => {
      report.reset();
      refresh();
    });
  });

  const usable = $derived(report.data ? report.data.maxConnections - report.data.reserved : 0);
  const level = $derived(report.data ? connectionLevel(report.data.total, usable) : "ok");
  const hidden = $derived(report.data?.groups.some((g) => g.user === null) ?? false);

  const columns = $derived<Column<ConnectionGroup>[]>([
    {
      key: "app",
      header: t("Aplicación"),
      width: 220,
      value: (g) => g.application ?? (g.user === null ? t("(sin permiso para verlas)") : t("(sin nombre)")),
    },
    { key: "user", header: t("Rol"), width: 140, value: (g) => g.user ?? "—" },
    { key: "addr", header: t("Dirección"), width: 150, value: (g) => g.address ?? t("local") },
    {
      key: "total",
      header: t("Total"),
      width: 80,
      align: "right",
      value: (g) => count(g.total),
      sort: (g) => g.total,
    },
    {
      key: "active",
      header: t("Activas"),
      width: 90,
      align: "right",
      value: (g) => count(g.active),
      sort: (g) => g.active,
    },
    {
      key: "idle",
      header: t("Inactivas"),
      width: 100,
      align: "right",
      value: (g) => count(g.idle),
      sort: (g) => g.idle,
      tone: (g) => (g.total >= 5 && g.idle / g.total >= 0.8 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
    {
      key: "tx",
      header: t("En transacción"),
      width: 130,
      align: "right",
      value: (g) => count(g.idleInTransaction),
      sort: (g) => g.idleInTransaction,
      tone: (g) => (g.idleInTransaction > 0 ? "text-amber-600 dark:text-amber-400" : undefined),
    },
    {
      key: "oldest",
      header: t("Inactiva más vieja"),
      width: 160,
      align: "right",
      value: (g) => duration(g.oldestIdleSeconds),
      sort: (g) => g.oldestIdleSeconds ?? -1,
    },
  ]);
</script>

<PanelFrame
  title={t("Conexiones")}
  icon="plug"
  loading={report.loading}
  loadedAt={report.loadedAt}
  error={report.error}
  onrefresh={refresh}
>
  <div class="flex h-full flex-col gap-3 p-3">
    {#if report.data}
      {@const r = report.data}
      <div class="flex flex-col gap-1">
        <div class="flex items-baseline gap-2 text-xs">
          <span class="text-base font-semibold">{count(r.total)}</span>
          <span class="muted">{t("de {n} disponibles", { n: count(usable) })}</span>
          <span class="muted">· {t("máximo {max}, {reserved} reservadas a superusuarios", { max: count(r.maxConnections), reserved: count(r.reserved) })}</span>
        </div>
        <div class="h-2 rounded bg-zinc-200 dark:bg-zinc-700">
          <div
            class="h-2 rounded {level === 'bad' ? 'bg-rose-500' : level === 'warn' ? 'bg-amber-500' : 'bg-emerald-500'}"
            style:width="{usable > 0 ? Math.min(100, (r.total / usable) * 100) : 0}%"
            title={percent(usable > 0 ? r.total / usable : 0)}
          ></div>
        </div>
      </div>
      {#if hidden}
        <p class="text-xs muted">
          {t("Este rol no ve el detalle de las sesiones de otros roles: solo cuenta cuántas son.")}
          {t("Hace falta el rol")} <span class="font-mono">pg_read_all_stats</span> {t("para ver de quién son.")}
        </p>
      {/if}
    {/if}
    <div class="min-h-0 flex-1">
      <DataGrid
        {columns}
        rows={report.data?.groups ?? []}
        rowKey={(g) => `${g.application}|${g.user}|${g.address}`}
        sortable
        empty={t("No hay conexiones de usuarios.")}
      />
    </div>
  </div>
</PanelFrame>
