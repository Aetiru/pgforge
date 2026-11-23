<script lang="ts">
  import { t } from "./i18n.svelte";
  import ColumnFilterDialog from "./ColumnFilterDialog.svelte";
  import FkPicker from "./FkPicker.svelte";
  import PasteRowsDialog from "./PasteRowsDialog.svelte";
  import { addToWhere, type FilterOp } from "./column-filter";
  import { untrack } from "svelte";
  import { environmentOf, readOnlyReason } from "./access.svelte";
  import { envBar, envLook } from "./badges";
  import Alert from "./Alert.svelte";
  import DataGrid, { type Column } from "./DataGrid.svelte";
  import { explorer } from "./explorer.svelte";
  import { boolText, isBoolType } from "./format";
  import { geometryText, isGeometryType } from "./geometry";
  import { columnWidth } from "./grid-width";
  import { DEFAULT_GRID_FONT, gridZoom } from "./grid.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import RecordPanel from "./RecordPanel.svelte";
  import TableStructure, { type StructureSection } from "./TableStructure.svelte";
  import { PAGE_SIZES, paging } from "./paging.svelte";
  import { timeZone } from "./tz.svelte";
  import { numberFormat } from "./numfmt.svelte";
  import TimeZoneSelect from "./TimeZoneSelect.svelte";
  import { gridViews } from "./grid-views.svelte";
  import { columnNameOfKey } from "./grid-views";
  import { openData, type DataTab, type Row } from "./data.svelte";
  import { jumpWhere, linkFor, linkLabel } from "./fk-nav";
  import type { TableLink } from "./ipc";

  let { tab }: { tab: DataTab } = $props();

  /** Lo que se muestra en lugar de un NULL, que no es lo mismo que una celda vacía. */
  const NULL = "NULL";

  const SAMPLE = 50;
  const MIN_WIDTH = 72;
  const MAX_WIDTH = 340;

  // Anchos fijos para el esqueleto de "abriendo la tabla": se repiten con módulo si hay más
  // columnas de las que hay anchos, así ninguna franja se ve como si fuera un solo bloque.
  const SKELETON_WIDTHS = [90, 140, 70, 110, 60, 130];
  const SKELETON_COLUMNS = Array.from({ length: 8 });
  const SKELETON_ROWS = Array.from({ length: 10 });

  /** Tipos que se leen en columna, con las unidades alineadas: van a la derecha. */
  const NUMERIC = new Set([
    "smallint",
    "integer",
    "bigint",
    "int2",
    "int4",
    "int8",
    "numeric",
    "decimal",
    "real",
    "double precision",
    "float4",
    "float8",
    "money",
  ]);

  const shape = $derived(tab.shape);
  const readOnlyProfile = $derived(readOnlyReason(tab.profileId));

  const definitions = $derived.by<Column<Row>[]>(() => {
    if (!shape) return [];

    const sample = tab.rows.slice(0, SAMPLE);

    const cells: Column<Row>[] = shape.columns.map((column, index) => {
      // Acá el tipo lo dice el catálogo, así que no hace falta ningún interruptor encendido.
      const bool = isBoolType(column.typeName);
      // Mismo caso que el booleano: el tipo viene con confianza, así que no hace falta el
      // heurístico por valor que usa el resultado de una consulta (`ResultGrid`).
      const geometry = isGeometryType(column.typeName);
      const longest = sample.reduce(
        (max, row) => Math.max(max, (tab.value(row, index) ?? NULL).length),
        column.name.length,
      );
      // Por tipo del catálogo: `numeric(12,2)` también, aunque no esté en `NUMERIC` tal cual.
      const numericType = /^(smallint|integer|bigint|numeric|decimal|real|double precision|int[248]|float[48])\b/.test(
        column.typeName.toLowerCase(),
      );

      return {
        key: `${index}-${column.name}`,
        header: column.name,
        // El tipo va como texto secundario: al editar es lo que dice qué se puede escribir en la celda.
        caption: column.typeName,
        width: columnWidth({
          longest,
          fontSize: gridZoom.size,
          min: MIN_WIDTH,
          max: MAX_WIDTH,
          padding: 24,
        }),
        // Acá el tipo lo dice el catálogo, así que la alineación no hay que adivinarla del valor.
        align: NUMERIC.has(column.typeName.toLowerCase()) ? "right" : "left",
        // Con este nombre la ordena el servidor.
        orderBy: column.name,
        value: (row) => {
          const value = tab.value(row, index);
          if (value === null) return NULL;
          if (bool) return boolText(value);
          if (geometry) return geometryText(value);
          const shown = timeZone.show(value);
          return oneLine(numericType ? numberFormat.show(shown) : shown);
        },
        edit: (row) => tab.value(row, index),
        title: (row) => tab.value(row, index) ?? undefined,
        tone: (row) => {
          if (tab.value(row, index) === null) return "cell-null";
          return row.edited.has(index) ? "font-medium text-amber-700 dark:text-amber-400" : "";
        },
      };
    });

    return [
      {
        key: "#",
        header: "",
        // La marca de la fila (editada, nueva, borrada) más el aire: no lleva número, así que no
        // crece con la cantidad de filas.
        // La letra de la grilla es una preferencia (`gridZoom`, compartida con el resultado de
        // consultas y el dashboard): la marca crece con ella igual que el texto de al lado.
        width: Math.round(44 * (gridZoom.size / DEFAULT_GRID_FONT)),
        align: "right",
        value: (row) => marca(row),
        tone: (row) => {
          if (row.deleted) return "text-rose-600 dark:text-rose-400";
          if (row.isNew) return "text-emerald-600 dark:text-emerald-400";
          if (row.edited.size > 0) return "text-amber-600 dark:text-amber-400";
          return "text-zinc-400 dark:text-zinc-600";
        },
      },
      ...cells,
    ];
  });

  /** Un símbolo por fila dice más que una columna de números en una grilla que se está editando. */
  function marca(row: Row): string {
    if (row.deleted) return "✕";
    if (row.isNew) return "+";
    if (row.edited.size > 0) return "•";
    return String(row.index + 1);
  }

  function oneLine(value: string | null): string {
    if (value === null) return NULL;
    // El `includes` antes de la expresión regular no es una manía: `value()` se llama por cada
    // celda dibujada y en cada cuadro del desplazamiento, y la enorme mayoría de los valores no
    // tiene ningún salto de línea que aplastar.
    return value.includes("\n") ? value.replace(/\s*\n\s*/g, " ↵ ") : value;
  }

  /** La clave de la columna arranca con su posición; la del marcador no es un número. */
  function columnIndex(column: Column<Row>): number {
    return Number(column.key.split("-")[0]);
  }

  function editable(row: Row, column: Column<Row>): boolean {
    if (!tab.editable || row.deleted) return false;

    const definition = shape?.columns[columnIndex(column)];
    // Las generadas y las de identidad las calcula el servidor: ofrecer editarlas sería mentir.
    return definition !== undefined && !definition.generated;
  }

  /**
   * Lo escrito en la caja del `WHERE` antes de aplicarlo. Va aparte de `tab.filter` —lo que el
   * servidor está usando— porque tipear no puede releer la tabla en cada tecla.
   */
  let draftFilter = $state(untrack(() => tab.filter));

  /**
   * Lo escrito en la caja de búsqueda antes de aplicarla. Mismo patrón que `draftFilter`: tipear
   * no puede releer la tabla en cada tecla.
   */
  let draftSearch = $state(untrack(() => tab.search));

  /**
   * Ordenar o filtrar vuelve a leer desde la primera tanda, y eso se llevaría puestas las ediciones
   * sin guardar. En vez de perderlas en silencio, los controles se apagan y dicen por qué.
   */
  const pendingBlocks = $derived(
    tab.pending > 0 ? t("Hay cambios sin guardar: guardalos o descartalos antes de volver a leer") : null,
  );

  /** Contra qué entorno se está editando. Pinta la barra igual que en la pestaña de consulta:
      guardar una fila cambia datos, y saber dónde no puede depender de recordar de qué servidor se
      abrió la grilla. */
  const environment = $derived(environmentOf(tab.profileId));

  /** El filtro por columna que se está armando, o `null` si el diálogo está cerrado. */
  let columnFilter = $state<{ column: string; preset?: { op: FilterOp; value?: string } } | null>(
    null,
  );

  function askColumnFilter(column: Column<Row>, preset?: { op: FilterOp; value?: string }) {
    if (pendingBlocks !== null || !column.orderBy) return;
    columnFilter = { column: column.orderBy, preset };
  }

  /** Suma la condición al `WHERE` escrito y vuelve a leer, igual que Enter en la caja. */
  function applyColumnFilter(sql: string) {
    draftFilter = addToWhere(draftFilter, sql);
    tab.applyFilter(draftFilter);
  }

  // Saltos por clave foránea. El mapa se arma una vez por forma de tabla: `linkAt` corre por celda
  // en cada cuadro del desplazamiento.
  const outgoing = $derived.by(() => {
    const map = new Map<string, TableLink | null>();
    for (const column of shape?.columns ?? []) {
      map.set(column.name, linkFor(shape?.foreignKeys ?? [], column.name));
    }
    return map;
  });

  function jump(link: TableLink, row: Row) {
    const current = shape;
    if (!current) return;
    const where = jumpWhere(link, (name) => {
      const at = current.columns.findIndex((column) => column.name === name);
      return at === -1 ? undefined : tab.value(row, at);
    });
    if (where === null) return;
    void openData(tab.profileId, tab.database, linkLabel(link), link.oid, where);
  }

  function linkAt(row: Row, column: Column<Row>) {
    if (row.isNew) return null;
    const name = shape?.columns[columnIndex(column)]?.name;
    const link = name === undefined ? null : outgoing.get(name);
    if (!link) return null;
    const where = jumpWhere(link, (col) => {
      const at = shape!.columns.findIndex((c) => c.name === col);
      return at === -1 ? undefined : tab.value(row, at);
    });
    return where === null ? null : { label: linkLabel(link), open: () => jump(link, row) };
  }

  /** «Ver filas que apuntan aquí»: una entrada por clave foránea de otra tabla. */
  function rowMenu(row: Row, column: Column<Row>) {
    const items: { label: string; run: () => void }[] = [];

    // Elegir de una lista: solo en una celda de clave foránea que se puede editar.
    const name = shape?.columns[columnIndex(column)]?.name;
    const link = name === undefined ? null : outgoing.get(name);
    if (link && editable(row, column)) {
      items.push({
        label: t("Elegir de la lista de {name}…", { name: linkLabel(link) }),
        run: () => (fkPick = { row, link }),
      });
    }

    if (!row.isNew) {
      for (const incoming of shape?.referencedBy ?? []) {
        items.push({
          label: t("Ver filas de {name} que apuntan aquí ({columns})", {
            name: linkLabel(incoming),
            columns: incoming.otherColumns.join(", "),
          }),
          run: () => jump(incoming, row),
        });
      }
    }
    return items;
  }

  let pasteOpen = $state(false);

  /** La clave foránea que se está eligiendo de una lista, y de qué fila. */
  let fkPick = $state<{ row: Row; link: TableLink } | null>(null);

  /** Pone en las columnas propias los valores elegidos, en el orden de la clave. */
  function applyPick(pick: { row: Row; link: TableLink }, values: Record<string, string | null>) {
    const current = shape;
    if (!current) return;
    pick.link.columns.forEach((own, position) => {
      const at = current.columns.findIndex((column) => column.name === own);
      if (at !== -1) tab.edit(pick.row, at, values[pick.link.otherColumns[position]] ?? null);
    });
  }

  // Vistas guardadas: columnas visibles, orden y filtro con nombre, por tabla.
  let grid = $state<{ hiddenKeys(): string[]; setHiddenKeys(keys: string[]): void }>();
  const savedViews = $derived(
    shape ? gridViews.of(tab.profileId, tab.database, shape.schema, shape.name) : [],
  );
  let appliedView = $state("");
  let namingView = $state(false);
  let viewName = $state("");

  function saveView() {
    const current = shape;
    const name = viewName.trim();
    if (!current || name === "") return;
    gridViews.save(tab.profileId, tab.database, current.schema, current.name, {
      name,
      hidden: (grid?.hiddenKeys() ?? []).map(columnNameOfKey),
      order: tab.order,
      filter: tab.filter,
    });
    appliedView = name;
    namingView = false;
    viewName = "";
  }

  async function applyView(name: string) {
    const view = savedViews.find((item) => item.name === name);
    appliedView = name;
    if (!view) return;
    tab.order = view.order;
    tab.filter = view.filter;
    draftFilter = view.filter;
    await tab.load();
    const keys = definitions
      .filter((column) => view.hidden.includes(columnNameOfKey(column.key)))
      .map((column) => column.key);
    grid?.setHiddenKeys(keys);
  }

  function removeView() {
    const current = shape;
    if (!current || appliedView === "") return;
    gridViews.remove(tab.profileId, tab.database, current.schema, current.name, appliedView);
    appliedView = "";
  }

  const views = $derived<{ id: "data" | StructureSection; label: string }[]>([
    { id: "data", label: t("Datos") },
    { id: "columns", label: t("Columnas") },
    { id: "indexes", label: t("Índices") },
    { id: "constraints", label: t("Restricciones") },
    { id: "triggers", label: "Triggers" },
    { id: "stats", label: t("Estadísticas") },
    { id: "dependencies", label: t("Dependencias") },
    { id: "ddl", label: "DDL" },
  ]);
  let view = $state<"data" | StructureSection>("data");

  /** La fila sobre la que actúan los botones de la barra. */
  let selected = $state<number | null>(null);
  const selectedRow = $derived(tab.rows.find((row) => row.index === selected) ?? null);

  // Ver la fila entera, de arriba abajo. Se recuerda como preferencia de la aplicación: quien lo
  // enciende para leer una tabla ancha lo quiere en la siguiente también.
  const RECORD_KEY = "pgforge.data.record";
  let showRecord = $state(
    (() => {
      try {
        return localStorage.getItem(RECORD_KEY) === "1";
      } catch {
        return false;
      }
    })(),
  );

  function toggleRecord() {
    showRecord = !showRecord;
    try {
      localStorage.setItem(RECORD_KEY, showRecord ? "1" : "0");
    } catch {
      // Sin `localStorage` no se recuerda esta vez.
    }
  }

  // Sin fila elegida se muestra la primera: abrir el panel y encontrarlo vacío parece roto.
  const recordRow = $derived(selectedRow ?? tab.rows[0] ?? null);
</script>

<div class="flex h-full flex-col">
  <!-- Las pestañas de adentro: la tabla y todo lo que la describe, sin salir de la pestaña ni abrir
       el panel de detalle. «Datos» es la grilla de siempre; el resto es de solo lectura. -->
  <div class="divider-b flex items-center gap-2 overflow-x-auto px-2 py-1">
    <div class="seg shrink-0" role="tablist">
      {#each views as item (item.id)}
        <button
          class="seg-item"
          role="tab"
          aria-selected={view === item.id}
          onclick={() => (view = item.id)}
        >
          {item.label}
        </button>
      {/each}
    </div>
  </div>

  {#if view !== "data"}
    <div class="min-h-0 flex-1">
      <TableStructure {tab} section={view} />
    </div>
  {:else}
  <header class="toolbar {envBar(environment)}">
    <button class="btn" disabled={tab.loading || tab.saving} onclick={() => tab.load()}>
      <Icon name="refresh" size={12} />
      {t("Refrescar")}
    </button>

    <!-- Llegar desde la grilla hasta dónde vive la tabla eran cinco niveles a mano cada vez. -->
    <button
      class="btn btn-ghost btn-icon"
      disabled={tab.shape === null}
      title={t("Revelar la tabla en el árbol")}
      aria-label={t("Revelar en el árbol")}
      onclick={() =>
        tab.shape &&
        explorer.revealRelation(tab.profileId, tab.database, tab.shape.schema, tab.shape.oid)}
    >
      <Icon name="compass" size={12} />
    </button>

    {#if tab.editable}
      <span class="toolbar-sep"></span>

      <button
        class="btn"
        disabled={tab.saving}
        title={t("Pega filas copiadas de Excel como filas nuevas")}
        onclick={() => (pasteOpen = true)}
      >
        {t("Pegar filas…")}
      </button>
      <button class="btn" disabled={tab.saving} onclick={() => tab.addRow()}>
        <Icon name="plus" size={12} />
        {t("Fila")}
      </button>
      <button
        class="btn {selectedRow?.deleted ? '' : 'btn-danger-ghost'}"
        disabled={selectedRow === null || tab.saving}
        title={t("Marca la fila elegida para borrarla al guardar")}
        onclick={() => selectedRow && tab.toggleDelete(selectedRow)}
      >
        <Icon name="trash" size={12} />
        {selectedRow?.deleted ? t("No borrar") : t("Borrar fila")}
      </button>

      <span class="toolbar-sep"></span>

      <button
        class="btn"
        disabled={tab.pending === 0 || tab.saving}
        title={t("Muestra el SQL exacto que se va a ejecutar")}
        onclick={() => tab.showPreview()}
      >
        <Icon name="sql" size={12} />
        {t("Ver el SQL")}
      </button>
      <button class="btn" disabled={tab.pending === 0 || tab.saving} onclick={() => tab.discard()}>
        {t("Descartar")}
      </button>
      <button
        class="btn btn-primary"
        disabled={tab.pending === 0 || tab.saving}
        onclick={() => tab.save()}
      >
        {#if tab.saving}<span class="spinner"></span>{/if}
        {t("Guardar")}
        {#if tab.pending > 0}
          <span class="rounded bg-white/20 px-1 text-[11px] tabular-nums">{tab.pending}</span>
        {/if}
      </button>
    {/if}

    <span class="ml-auto flex items-center gap-2 text-xs muted">
      <button
        class="btn btn-sm"
        aria-pressed={showRecord}
        title={t("Muestra todos los valores de la fila elegida, una columna por línea")}
        onclick={toggleRecord}
      >
        <Icon name="columns" size={12} />
        {t("Fila completa")}
      </button>

      {#if tab.loading}
        <span class="spinner"></span>
      {/if}

      <!-- La franja de la barra pinta el entorno, pero producción lleva además su pastilla: al color
           solo no se le puede confiar la única distinción que importa de verdad. -->
      {#if environment}
        {@const badge = envLook(environment)}
        <span class="tag {badge.tone}" title={badge.title}>{badge.label}</span>
      {/if}

      <!-- Vistas guardadas de esta tabla. Aplicar una vuelve a leer: cambia el orden y el filtro. -->
      {#if namingView}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="field w-36 py-0.5 text-xs"
          placeholder={t("Nombre de la vista")}
          maxlength="40"
          autofocus
          bind:value={viewName}
          onkeydown={(event) => {
            if (event.key === "Enter") saveView();
            if (event.key === "Escape") namingView = false;
          }}
        />
        <button class="btn btn-sm" disabled={viewName.trim() === ""} onclick={saveView}>{t("Guardar")}</button>
        <button class="btn btn-sm btn-ghost" onclick={() => (namingView = false)}>{t("Cancelar")}</button>
      {:else}
        {#if savedViews.length > 0}
          <label class="flex items-center gap-1" title={t("Vistas guardadas de esta tabla")}>
            <span>{t("Vista")}</span>
            <select
              class="field py-0.5 text-xs"
              value={appliedView}
              disabled={pendingBlocks !== null || tab.loading}
              onchange={(event) => void applyView(event.currentTarget.value)}
            >
              <option value="">—</option>
              {#each savedViews as item (item.name)}
                <option value={item.name}>{item.name}</option>
              {/each}
            </select>
          </label>
        {/if}
        <button
          class="btn btn-sm btn-ghost"
          title={t("Guarda las columnas visibles, el orden y el filtro de ahora con un nombre")}
          onclick={() => (namingView = true)}>{t("Guardar vista…")}</button
        >
        {#if appliedView !== ""}
          <button class="btn btn-sm btn-ghost" title={t("Borra la vista elegida")} onclick={removeView}>
            {t("Borrar vista")}
          </button>
        {/if}
      {/if}

      <TimeZoneSelect />

      <!-- Cuántas filas trae cada tanda. La tabla no se lee entera nunca: el scroll pide la
           siguiente al acercarse al final, y el tamaño depende de para qué se abrió. -->
      <label class="flex items-center gap-1" title={t("Filas que trae cada tanda")}>
        <span>{t("Tanda")}</span>
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

      <!-- Pasado el techo, seguir trayendo es decisión del usuario y no de la barra de scroll. -->
      {#if !tab.complete && tab.autoDone}
        <button
          class="btn btn-sm"
          disabled={tab.loading}
          title={t("El scroll ya trajo todo lo que trae solo")}
          onclick={() => tab.more()}
        >
          {t("Cargar {n} más", { n: paging.size })}
        </button>
      {/if}

      <span class="tabular-nums">
        {#if tab.complete}
          {tab.rows.length === 1 ? t("{n} fila", { n: tab.rows.length }) : t("{n} filas", { n: tab.rows.length })}
        {:else}
          {tab.rows.length === 1 ? t("{n} fila o más", { n: tab.rows.length }) : t("{n} filas o más", { n: tab.rows.length })}
        {/if}
      </span>
      {#if shape?.key}
        <span class="tag tag-neutral" title={t("Con esta clave se identifica cada fila al guardar")}>
          <Icon name="key" size={10} />
          {shape.key.columns.join(", ")}
        </span>
      {/if}
    </span>
  </header>

  <!--
    Dos mecanismos distintos, a propósito. El `WHERE` es la expresión que escribe el usuario, va
    cruda a la consulta —misma frontera de confianza que el editor de SQL— y la valida el servidor
    al ejecutar. La búsqueda es un `OR` de `ILIKE` que arma el núcleo y viaja parametrizada: no hay
    SQL que escribir ni riesgo de inyección, a cambio de que solo busca texto literal en cualquier
    columna. Las dos resuelven en el servidor y no en lo ya traído —sobre una tabla grande, «las
    que contienen X» no está entre las doscientas que se leyeron primero—, así que las dos vuelven
    a leer desde la primera tanda y se aplican con Enter y se limpian con Escape.
  -->
  <div class="divider-b flex flex-col gap-1 px-2 py-1">
    <div class="flex items-center gap-2">
      <span class="shrink-0 font-mono text-xs muted">WHERE</span>
      <input
        class="field min-w-0 flex-1 py-0.5 font-mono text-xs"
        placeholder="estado = 'activo' AND creado &gt; now() - interval '7 days'"
        title={pendingBlocks ?? t("Se aplica con Enter; vuelve a leer la tabla desde la primera tanda")}
        disabled={pendingBlocks !== null}
        bind:value={draftFilter}
        onkeydown={(event) => {
          if (event.key === "Enter") tab.applyFilter(draftFilter);
          else if (event.key === "Escape") {
            draftFilter = "";
            if (tab.filter !== "") tab.applyFilter("");
          }
        }}
      />
      <button
        class="btn btn-sm"
        disabled={pendingBlocks !== null || tab.loading || draftFilter.trim() === tab.filter.trim()}
        onclick={() => tab.applyFilter(draftFilter)}
      >
        {t("Aplicar")}
      </button>
      {#if tab.filter.trim() !== ""}
        <button
          class="btn btn-sm"
          disabled={pendingBlocks !== null || tab.loading}
          onclick={() => {
            draftFilter = "";
            tab.applyFilter("");
          }}
        >
          {t("Quitar")}
        </button>
      {/if}
    </div>

    <div class="flex items-center gap-2">
      <span class="shrink-0" title={t("Busca texto literal en todas las columnas")}>
        <Icon name="search" size={12} />
      </span>
      <input
        class="field min-w-0 flex-1 py-0.5 text-xs"
        placeholder={t("Buscar en cualquier columna…")}
        title={pendingBlocks ?? t("Se aplica con Enter; vuelve a leer la tabla desde la primera tanda")}
        disabled={pendingBlocks !== null}
        bind:value={draftSearch}
        onkeydown={(event) => {
          if (event.key === "Enter") tab.applySearch(draftSearch);
          else if (event.key === "Escape") {
            draftSearch = "";
            if (tab.search !== "") tab.applySearch("");
          }
        }}
      />
      <button
        class="btn btn-sm"
        disabled={pendingBlocks !== null || tab.loading || draftSearch.trim() === tab.search.trim()}
        onclick={() => tab.applySearch(draftSearch)}
      >
        {t("Aplicar")}
      </button>
      {#if tab.search.trim() !== ""}
        <button
          class="btn btn-sm"
          disabled={pendingBlocks !== null || tab.loading}
          onclick={() => {
            draftSearch = "";
            tab.applySearch("");
          }}
        >
          {t("Quitar")}
        </button>
      {/if}
    </div>
  </div>

  <!-- Dos motivos distintos por los que no se puede editar; el del perfil manda porque vale para
       cualquier tabla que se abra. -->
  {#if readOnlyProfile}
    <Alert tone="warn">{readOnlyProfile}</Alert>
  {:else if shape?.readOnly}
    <Alert tone="warn">{shape.readOnly}</Alert>
  {/if}

  {#if tab.error}
    <Alert tone="bad" onclose={() => (tab.error = null)}>{tab.error}</Alert>
  {/if}

  <div class="flex min-h-0 flex-1">
  <div class="min-h-0 min-w-0 flex-1">
    {#if !shape}
      <div class="flex flex-col gap-px p-3" aria-label={t("Abriendo la tabla…")}>
        <div class="flex gap-2 pb-2">
          {#each SKELETON_COLUMNS as _, column (column)}
            <div
              class="skeleton h-4"
              style="width: {SKELETON_WIDTHS[column % SKELETON_WIDTHS.length]}px"
            ></div>
          {/each}
        </div>
        {#each SKELETON_ROWS as _, row (row)}
          <div class="flex gap-2 py-1.5">
            {#each SKELETON_COLUMNS as _, column (column)}
              <div
                class="skeleton h-3"
                style="width: {SKELETON_WIDTHS[column % SKELETON_WIDTHS.length]}px"
              ></div>
            {/each}
          </div>
        {/each}
      </div>
    {:else}
      <DataGrid
        bind:this={grid}
        columns={definitions}
        rows={tab.rows}
        rowKey={(row) => row.index}
        selectedKey={selected}
        empty={t("La tabla está vacía.")}
        editable={tab.editable ? editable : undefined}
        onedit={(row, column, value) => tab.edit(row, columnIndex(column), value)}
        onnearend={() => tab.moreAuto()}
        sortable={pendingBlocks === null}
        serverSort
        onfilter={pendingBlocks === null ? askColumnFilter : undefined}
        {linkAt}
        {rowMenu}
        onsort={(column, descending) => tab.setOrder(column?.orderBy ?? null, descending)}
        rowClass={(row) =>
          row.deleted
            ? "line-through opacity-50"
            : row.isNew
              ? "bg-emerald-50/60 dark:bg-emerald-950/30"
              : ""}
        onselect={(row) => (selected = row.index)}
      />
    {/if}
  </div>
  {#if showRecord && shape}
    <RecordPanel {tab} row={recordRow} onclose={toggleRecord} />
  {/if}
  </div>

  {#if tab.editable && tab.rows.length > 0}
    <p class="divider-t flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-1 text-xs muted">
      <span><span class="kbd">{t("Doble clic")}</span> {t("en una celda para editarla")}</span>
      <span><span class="kbd">∅</span> {t("pone NULL")}</span>
      <span><span class="kbd">Enter</span> {t("confirma")} · <span class="kbd">Esc</span> {t("descarta")}</span>
      <span class="ml-auto">{t("Nada se escribe en la base hasta que se guarda.")}</span>
    </p>
  {/if}
  {/if}
</div>

{#if tab.preview}
  <Modal
    title={t("Esto es lo que se va a ejecutar")}
    subtitle={tab.preview.length === 1
      ? t("La sentencia va en una sola transacción: si falla, no se aplica.")
      : t("Las {n} sentencias van en una sola transacción: si alguna falla, no se aplica ninguna.", { n: tab.preview.length })}
    size="xl"
    busy={tab.saving}
    onclose={() => (tab.preview = null)}
  >
    {#each tab.preview as statement, index (index)}
      <div class="mb-2 rounded-md border border-zinc-200 p-2 dark:border-zinc-700">
        <pre class="font-mono text-xs whitespace-pre-wrap select-text">{statement.sql}</pre>
        {#if statement.params.length > 0}
          <div class="mt-1 flex flex-wrap gap-1.5 text-[11px] muted">
            {#each statement.params as param, position (position)}
              <span class="rounded bg-zinc-100 px-1.5 py-px font-mono dark:bg-zinc-700">
                ${position + 1} = {param === null ? "NULL" : param}
              </span>
            {/each}
          </div>
        {/if}
      </div>
    {/each}

    {#snippet footer()}
      <button class="btn ml-auto" onclick={() => (tab.preview = null)}>{t("Cerrar")}</button>
      <button class="btn btn-primary" disabled={tab.saving} onclick={() => tab.save()}>
        {#if tab.saving}<span class="spinner"></span>{/if}
        {t("Guardar")}
      </button>
    {/snippet}
  </Modal>
{/if}

{#if pasteOpen && shape}
  <PasteRowsDialog
    columns={shape.columns.map((column) => ({ name: column.name, generated: column.generated }))}
    onadd={(rows) => tab.addRows(rows)}
    onclose={() => (pasteOpen = false)}
  />
{/if}

{#if fkPick}
  {@const pick = fkPick}
  <FkPicker
    profileId={tab.profileId}
    database={tab.database}
    link={pick.link}
    current={pick.link.columns.map((own) => {
      const at = shape?.columns.findIndex((column) => column.name === own) ?? -1;
      return at === -1 ? null : tab.value(pick.row, at);
    })}
    onpick={(values) => applyPick(pick, values)}
    onclose={() => (fkPick = null)}
  />
{/if}

{#if columnFilter && shape}
  <ColumnFilterDialog
    column={columnFilter.column}
    columns={shape.columns.map((item) => item.name)}
    types={shape.columns.map((item) => item.typeName)}
    preset={columnFilter.preset}
    onapply={applyColumnFilter}
    onclose={() => (columnFilter = null)}
  />
{/if}
