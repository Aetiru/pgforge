<script lang="ts">
  import { untrack } from "svelte";
  import DataGrid, { type Column } from "../DataGrid.svelte";
  import Empty from "../Empty.svelte";
  import { bytes, count } from "../format";
  import { spaceOverview, type Growth, type SpaceOverview } from "../ipc";
  import { growthText } from "../diagnose-view";
  import { squarify } from "../treemap";
  import { Loader } from "./loader.svelte";
  import PanelFrame from "./PanelFrame.svelte";

  /**
   * Mapa de espacio de la base: un rectángulo por esquema (clic para entrar) y, adentro, uno por
   * tabla con su TOAST y sus índices. Abajo, cuánto crece cada cosa según las muestras guardadas.
   */
  let { profileId, database }: { profileId: string; database: string | null } = $props();

  const overview = new Loader<SpaceOverview>();

  function refresh() {
    void overview.load(() => spaceOverview(profileId));
  }

  /** Esquema en el que se entró; `null` es el nivel de los esquemas. */
  let schema = $state<string | null>(null);

  $effect(() => {
    void database;
    void profileId;
    untrack(() => {
      overview.reset();
      schema = null;
      refresh();
    });
  });

  interface Cell {
    key: string;
    label: string;
    bytes: number;
    hue: number;
    title: string;
    enter: string | null;
  }

  // La proporción de la caja fija la forma de los rectángulos: el contenedor usa la misma.
  const BOX = { x: 0, y: 0, w: 160, h: 100 };

  const cells = $derived.by<Cell[]>(() => {
    const map = overview.data?.map;
    if (!map) return [];
    if (schema === null) {
      const bySchema = new Map<string, number>();
      for (const e of map.entries) {
        bySchema.set(e.schema, (bySchema.get(e.schema) ?? 0) + e.heapBytes + e.toastBytes + e.indexBytes);
      }
      const list: Cell[] = [...bySchema.entries()]
        .sort((a, b) => b[1] - a[1])
        .map(([name, size], i) => ({
          key: name,
          label: name,
          bytes: size,
          hue: (i * 47 + 210) % 360,
          title: `${name}: ${bytes(size)}. Clic para ver sus tablas`,
          enter: name,
        }));
      if (map.restBytes > 0) {
        list.push({
          key: "~rest",
          label: "Catálogo y tablas menores",
          bytes: map.restBytes,
          hue: -1,
          title: `${bytes(map.restBytes)} que no están en el mapa: catálogo, tablas más chicas que el corte`,
          enter: null,
        });
      }
      return list;
    }
    const inSchema = map.entries.filter((e) => e.schema === schema);
    const index = [...new Set(map.entries.map((e) => e.schema))].indexOf(schema);
    return inSchema.map((e) => ({
      key: e.table,
      label: e.table,
      bytes: e.heapBytes + e.toastBytes + e.indexBytes,
      hue: (Math.max(index, 0) * 47 + 210) % 360,
      title: `${schema}.${e.table}: ${bytes(e.heapBytes + e.toastBytes + e.indexBytes)} (tabla ${bytes(e.heapBytes)}, TOAST y otros ${bytes(e.toastBytes)}, índices ${bytes(e.indexBytes)})`,
      enter: null,
    }));
  });

  const rects = $derived(squarify(cells.map((c) => c.bytes), BOX));
  const total = $derived(cells.reduce((sum, c) => sum + c.bytes, 0));

  const growthColumns: Column<Growth>[] = [
    { key: "name", header: "Nombre", width: 280, value: (g) => (g.kind === "database" ? `Base ${g.name}` : g.name) },
    {
      key: "size",
      header: "Tamaño",
      width: 110,
      align: "right",
      value: (g) => bytes(g.bytes),
      sort: (g) => g.bytes,
    },
    {
      key: "rate",
      header: "Crece",
      width: 150,
      align: "right",
      value: (g) => growthText(g.forecast?.perDay),
      sort: (g) => g.forecast?.perDay ?? Number.MIN_SAFE_INTEGER,
      title: (g) =>
        g.forecast
          ? `Recta sobre ${g.forecast.samples} muestras de ${g.forecast.spanDays.toFixed(1)} días`
          : "Se guarda una muestra por hora al abrir esta pestaña; hacen falta tres repartidas en medio día",
    },
    {
      key: "week",
      header: "Por semana",
      width: 120,
      align: "right",
      value: (g) => (g.forecast ? bytes(Math.abs(g.forecast.perDay) * 7) : "—"),
      sort: (g) => g.forecast?.perDay ?? Number.MIN_SAFE_INTEGER,
    },
    {
      key: "p30",
      header: "En 30 días",
      width: 120,
      align: "right",
      value: (g) => (g.projected30d === null ? "—" : bytes(g.projected30d)),
      sort: (g) => g.projected30d ?? 0,
    },
  ];
</script>

<PanelFrame
  title="Espacio"
  icon="database"
  loading={overview.loading}
  loadedAt={overview.loadedAt}
  error={overview.error}
  onrefresh={refresh}
>
  <div class="flex flex-col gap-3 p-3">
    {#if overview.data}
      {@const map = overview.data.map}
      <div class="flex flex-wrap items-center gap-2 text-xs">
        <button class="btn btn-sm" disabled={schema === null} onclick={() => (schema = null)}>
          {map.database} · {bytes(map.databaseBytes)}
        </button>
        {#if schema !== null}
          <span class="muted">›</span>
          <span class="font-mono">{schema}</span>
          <span class="muted">{bytes(total)}</span>
        {/if}
        <span class="ml-auto muted">
          Cada rectángulo es proporcional a su tamaño total (tabla, TOAST e índices).
        </span>
      </div>

      {#if cells.length === 0}
        <Empty icon="table" title="No hay tablas para mostrar" />
      {:else}
        <div
          class="relative w-full overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-700"
          style="aspect-ratio: {BOX.w} / {BOX.h}; max-height: 420px;"
        >
          {#each cells as cell, i (cell.key)}
            {@const r = rects[i]}
            {#if r.w > 0 && r.h > 0}
              <button
                class="absolute overflow-hidden border border-white/70 p-1 text-left text-[11px] leading-tight text-white focus-visible:outline-2 dark:border-zinc-900/70"
                class:cursor-pointer={cell.enter !== null}
                class:cursor-default={cell.enter === null}
                style:left="{(r.x / BOX.w) * 100}%"
                style:top="{(r.y / BOX.h) * 100}%"
                style:width="{(r.w / BOX.w) * 100}%"
                style:height="{(r.h / BOX.h) * 100}%"
                style:background={cell.hue < 0 ? "hsl(220 8% 45%)" : `hsl(${cell.hue} 50% 38%)`}
                title={cell.title}
                onclick={() => cell.enter && (schema = cell.enter)}
              >
                {#if r.w > 9 && r.h > 9}
                  <span class="block truncate font-semibold">{cell.label}</span>
                  <span class="block opacity-80">{bytes(cell.bytes)}</span>
                {/if}
              </button>
            {/if}
          {/each}
        </div>
      {/if}

      <h3 class="mt-1 text-xs font-semibold">Crecimiento</h3>
      <p class="text-xs muted">
        Se calcula con las muestras de tamaño que se guardan al abrir esta pestaña, una por hora como
        mucho. Sin muestras suficientes aparece «faltan muestras».
      </p>
      <div class="h-64">
        <DataGrid
          columns={growthColumns}
          rows={overview.data.growth}
          rowKey={(g) => `${g.kind}:${g.name}`}
          sortable
          empty="Todavía no hay muestras."
        />
      </div>
      <p class="text-xs muted">
        {count(map.entries.length)} tablas en el mapa
        {#if map.restBytes > 0}· {bytes(map.restBytes)} fuera de él (catálogo y tablas menores){/if}.
      </p>
    {:else if !overview.error}
      <Empty icon="database" title="Leyendo el espacio de la base…" />
    {/if}
  </div>
</PanelFrame>
