<script lang="ts">
  import Skeleton from "./Skeleton.svelte";
  /**
   * Una base abierta como pestaña: propiedades, esquemas, tablas y extensiones.
   *
   * De solo lectura. Lo que se hace con una tabla de la lista —abrir sus datos, consultarla— usa los
   * mismos caminos que el árbol; cambiar la base (renombrarla, borrarla, privilegios) sigue siendo
   * del panel de detalle, que es donde están los diálogos y la confirmación de producción.
   */
  import Alert from "./Alert.svelte";
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import { bytes, count } from "./format";
  import type { ExtensionSummary, SchemaSummary, TableSummary } from "./ipc";
  import type { DatabaseTab } from "./database.svelte";
  import { openData } from "./data.svelte";
  import { openQuery } from "./query.svelte";

  let { tab }: { tab: DatabaseTab } = $props();

  type View = "info" | "schemas" | "tables" | "extensions";

  let view = $state<View>("info");
  let selectedTable = $state<TableSummary | null>(null);
  let schemaFilter = $state<string>("");
  let needle = $state("");
  let actionError = $state<string | null>(null);

  const overview = $derived(tab.overview);
  const details = $derived(overview?.details ?? null);

  const VIEWS = $derived<{ id: View; label: string; count: number | null }[]>([
    { id: "info", label: "Información", count: null },
    { id: "schemas", label: "Esquemas", count: overview?.schemas.length ?? null },
    { id: "tables", label: "Tablas", count: overview?.tables.length ?? null },
    { id: "extensions", label: "Extensiones", count: overview?.extensions.length ?? null },
  ]);

  const KIND: Record<string, string> = {
    r: "tabla",
    p: "particionada",
    v: "vista",
    m: "materializada",
    f: "externa",
  };

  const properties = $derived.by<[string, string][]>(() => {
    if (!details) return [];
    return [
      ["Nombre", details.name],
      ["Propietario", details.owner],
      ["Tamaño", bytes(details.sizeBytes)],
      ["Codificación", details.encoding],
      ["Colación", details.collation],
      ["Tipo de carácter", details.ctype],
      ["Espacio de tablas", details.tablespace],
      [
        "Conexiones",
        details.connections === null
          ? "—"
          : `${details.connections}${details.connectionLimit >= 0 ? ` / ${details.connectionLimit}` : " (sin límite)"}`,
      ],
      ["Acepta conexiones", details.allowConnections ? "sí" : "no"],
      ["Plantilla", details.isTemplate ? "sí" : "no"],
      ["Esquemas", String(overview?.schemas.length ?? 0)],
      [
        "Tablas y vistas",
        `${count(overview?.tables.length ?? 0)}${overview?.tablesTruncated ? " o más" : ""}`,
      ],
      ["Extensiones", String(overview?.extensions.length ?? 0)],
    ];
  });

  const tables = $derived.by(() => {
    const wanted = needle.trim().toLowerCase();
    return (overview?.tables ?? []).filter(
      (item) =>
        (schemaFilter === "" || item.schema === schemaFilter) &&
        (wanted === "" || item.name.toLowerCase().includes(wanted)),
    );
  });

  const tableColumns: Column<TableSummary>[] = [
    { key: "schema", header: "Esquema", width: 130, value: (t) => t.schema },
    { key: "name", header: "Nombre", width: 260, value: (t) => t.name },
    { key: "kind", header: "Tipo", width: 120, value: (t) => KIND[t.kind] ?? t.kind },
    {
      key: "rows",
      header: "Filas (est.)",
      width: 120,
      align: "right",
      value: (t) => (t.kind === "v" ? "—" : count(Math.round(t.estimatedRows))),
      sort: (t) => t.estimatedRows,
    },
    {
      key: "size",
      header: "Tamaño",
      width: 110,
      align: "right",
      value: (t) => (t.kind === "v" ? "—" : bytes(t.totalBytes)),
      sort: (t) => t.totalBytes,
    },
  ];

  const schemaColumns: Column<SchemaSummary>[] = [
    { key: "name", header: "Esquema", width: 220, value: (s) => s.name },
    { key: "owner", header: "Propietario", width: 160, value: (s) => s.owner },
    {
      key: "tables",
      header: "Tablas",
      width: 100,
      align: "right",
      value: (s) => count(s.tables),
      sort: (s) => s.tables,
    },
    {
      key: "views",
      header: "Vistas",
      width: 100,
      align: "right",
      value: (s) => count(s.views),
      sort: (s) => s.views,
    },
  ];

  const extensionColumns: Column<ExtensionSummary>[] = [
    { key: "name", header: "Extensión", width: 220, value: (e) => e.name },
    { key: "version", header: "Versión", width: 120, value: (e) => e.version },
    { key: "schema", header: "Esquema", width: 160, value: (e) => e.schema },
  ];

  async function openSelected() {
    const item = selectedTable;
    if (!item) return;
    actionError = null;
    try {
      await openData(tab.profileId, tab.database, item.name, item.oid);
    } catch (error) {
      actionError = String(error);
    }
  }

  async function queryAll() {
    const item = selectedTable;
    if (!item) return;
    actionError = null;
    try {
      const query = await openQuery(tab.profileId, tab.database, item.name, false);
      query.sql = `SELECT * FROM "${item.schema}"."${item.name}" LIMIT 100;`;
    } catch (error) {
      actionError = String(error);
    }
  }

  const tableKey = (item: TableSummary) => item.oid;
</script>

<div class="flex h-full flex-col">
  <div class="divider-b flex items-center gap-2 overflow-x-auto px-2 py-1">
    <div class="seg shrink-0" role="tablist">
      {#each VIEWS as item (item.id)}
        <button
          class="seg-item"
          role="tab"
          aria-selected={view === item.id}
          onclick={() => (view = item.id)}
        >
          {item.label}
          {#if item.count !== null}
            <span class="seg-count">{item.count}</span>
          {/if}
        </button>
      {/each}
    </div>
    <button class="btn btn-sm ml-auto" disabled={tab.loading} onclick={() => tab.load()}>
      {#if tab.loading}
        <span class="spinner"></span>
      {:else}
        <Icon name="refresh" size={12} />
      {/if}
      Actualizar
    </button>
  </div>

  {#if tab.error}
    <Alert tone="bad" onclose={() => (tab.error = null)}>{tab.error}</Alert>
  {/if}
  {#if actionError}
    <Alert tone="bad" onclose={() => (actionError = null)}>{actionError}</Alert>
  {/if}

  <div class="min-h-0 flex-1">
    {#if !overview}
      {#if tab.loading}
        <Skeleton rows={6} label="Leyendo la base…" />
      {:else if !tab.error}
        <Empty icon="database" title="Sin datos" hint="No se pudo traer nada de la base todavía.">
          <button class="btn btn-primary" onclick={() => tab.load()}>
            <Icon name="refresh" size={12} /> Leer la base
          </button>
        </Empty>
      {/if}
    {:else if view === "info"}
      <div class="h-full overflow-auto p-4">
        <div class="mb-3 flex items-center gap-2">
          <Icon name="database" size={18} class="text-blue-600 dark:text-blue-400" />
          <h2 class="text-base font-semibold">{details?.name}</h2>
          {#if details?.isTemplate}<span class="tag tag-neutral">plantilla</span>{/if}
        </div>
        {#if details?.comment}
          <p class="mb-3 text-sm muted">{details.comment}</p>
        {/if}
        <dl class="grid max-w-xl grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm select-text">
          {#each properties as [label, value] (label)}
            <dt class="muted">{label}</dt>
            <dd class="font-mono">{value}</dd>
          {/each}
        </dl>
      </div>
    {:else if view === "schemas"}
      <DataGrid
        columns={schemaColumns}
        rows={overview.schemas}
        rowKey={(item) => item.name}
        sortable
        empty="Esta base no tiene esquemas propios."
        onselect={(item) => {
          schemaFilter = item.name;
          view = "tables";
        }}
      />
    {:else if view === "tables"}
      <div class="flex h-full flex-col">
        <div class="toolbar divider-b">
          <label class="check">
            Esquema
            <select class="field" bind:value={schemaFilter}>
              <option value="">Todos</option>
              {#each overview.schemas as item (item.name)}
                <option value={item.name}>{item.name}</option>
              {/each}
            </select>
          </label>
          <input
            class="field w-56 py-0.5 text-xs"
            placeholder="Filtrar por nombre…"
            bind:value={needle}
          />
          <span class="ml-auto text-xs muted">
            {count(tables.length)} de {count(overview.tables.length)}{overview.tablesTruncated
              ? " (hay más: se muestran las más pesadas)"
              : ""}
          </span>
          <button class="btn btn-sm btn-primary" disabled={!selectedTable} onclick={openSelected}>
            <Icon name="table" size={12} />
            Datos
          </button>
          <button class="btn btn-sm" disabled={!selectedTable} onclick={queryAll}>
            <Icon name="sql" size={12} />
            Consulta
          </button>
        </div>
        <div class="min-h-0 flex-1">
          <DataGrid
            columns={tableColumns}
            rows={tables}
            rowKey={tableKey}
            selectedKey={selectedTable ? tableKey(selectedTable) : null}
            onselect={(item) => (selectedTable = item)}
            sortable
            empty="Ninguna tabla coincide."
          />
        </div>
      </div>
    {:else}
      <DataGrid
        columns={extensionColumns}
        rows={overview.extensions}
        rowKey={(item) => item.name}
        sortable
        empty="No hay extensiones instaladas en esta base."
      />
    {/if}
  </div>
</div>
