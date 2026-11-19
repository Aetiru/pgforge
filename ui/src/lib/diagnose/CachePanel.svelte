<script lang="ts">
  import { untrack } from "svelte";
  import DataGrid, { type Column } from "../DataGrid.svelte";
  import { count, percent } from "../format";
  import { cacheStats, type CacheReport, type IndexCache, type TableCache } from "../ipc";
  import { Loader } from "./loader.svelte";
  import PanelFrame from "./PanelFrame.svelte";

  /**
   * Cuánto de lo que se lee viene de la caché de PostgreSQL. Se ordena por lecturas a disco y no por
   * porcentaje: un 50 % sobre cien bloques no es un problema, y un 95 % sobre mil millones sí.
   */
  let { profileId, database }: { profileId: string; database: string | null } = $props();

  const report = new Loader<CacheReport>();
  let view = $state<"tablas" | "indices">("tablas");

  function refresh() {
    void report.load(() => cacheStats(profileId));
  }

  $effect(() => {
    void database;
    void profileId;
    untrack(() => {
      report.reset();
      refresh();
    });
  });

  const warn = "text-amber-600 dark:text-amber-400";

  const tableColumns: Column<TableCache>[] = [
    { key: "table", header: "Tabla", width: 260, value: (t) => `${t.schema}.${t.table}` },
    {
      key: "reads",
      header: "Lecturas a disco",
      width: 150,
      align: "right",
      value: (t) => count(t.heapRead + t.indexRead),
      sort: (t) => t.heapRead + t.indexRead,
    },
    {
      key: "heap",
      header: "Aciertos tabla",
      width: 130,
      align: "right",
      value: (t) => percent(t.heapRatio),
      sort: (t) => t.heapRatio ?? 2,
      tone: (t) => (t.heapRatio !== null && t.heapRatio < 0.9 && t.level !== "ok" ? warn : undefined),
    },
    {
      key: "index",
      header: "Aciertos índices",
      width: 140,
      align: "right",
      value: (t) => percent(t.indexRatio),
      sort: (t) => t.indexRatio ?? 2,
      tone: (t) => (t.indexRatio !== null && t.indexRatio < 0.9 && t.level !== "ok" ? warn : undefined),
    },
    {
      key: "hits",
      header: "Aciertos (bloques)",
      width: 150,
      align: "right",
      value: (t) => count(t.heapHit + t.indexHit),
      sort: (t) => t.heapHit + t.indexHit,
    },
  ];

  const indexColumns: Column<IndexCache>[] = [
    { key: "index", header: "Índice", width: 240, value: (i) => `${i.schema}.${i.index}` },
    { key: "table", header: "Tabla", width: 200, value: (i) => i.table },
    {
      key: "reads",
      header: "Lecturas a disco",
      width: 150,
      align: "right",
      value: (i) => count(i.read),
      sort: (i) => i.read,
    },
    {
      key: "ratio",
      header: "Aciertos",
      width: 110,
      align: "right",
      value: (i) => percent(i.ratio),
      sort: (i) => i.ratio ?? 2,
      tone: (i) => (i.level !== "ok" ? warn : undefined),
    },
  ];
</script>

<PanelFrame
  title="Caché"
  icon="gauge"
  loading={report.loading}
  loadedAt={report.loadedAt}
  error={report.error}
  onrefresh={refresh}
>
  {#snippet actions()}
    <div class="seg" role="tablist">
      <button class="seg-item" role="tab" aria-selected={view === "tablas"} onclick={() => (view = "tablas")}>
        Tablas
      </button>
      <button class="seg-item" role="tab" aria-selected={view === "indices"} onclick={() => (view = "indices")}>
        Índices
      </button>
    </div>
  {/snippet}
  <div class="flex h-full flex-col">
    <p class="px-3 pt-3 text-xs muted">
      «Aciertos» es la parte de las lecturas que se resolvió en la caché de PostgreSQL. Una lectura a
      disco puede haberla resuelto igual la caché del sistema operativo, así que un porcentaje bajo
      es un motivo para mirar la tabla, no una medición del disco. Solo se marca con más de 10.000
      lecturas.
    </p>
    <div class="min-h-0 flex-1 p-3">
      {#if view === "tablas"}
        <DataGrid
          columns={tableColumns}
          rows={report.data?.tables ?? []}
          rowKey={(t) => `${t.schema}.${t.table}`}
          sortable
          empty="No hay estadísticas de E/S de tablas."
        />
      {:else}
        <DataGrid
          columns={indexColumns}
          rows={report.data?.indexes ?? []}
          rowKey={(i) => `${i.schema}.${i.index}`}
          sortable
          empty="No hay estadísticas de E/S de índices."
        />
      {/if}
    </div>
  </div>
</PanelFrame>
