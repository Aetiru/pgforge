<script lang="ts">
  import ColumnDialog from "./ColumnDialog.svelte";
  import Icon from "./Icon.svelte";
  import TableDialog from "./TableDialog.svelte";
  import { kindLabel, lookOf } from "./badges";
  import { explorer, type Row } from "./explorer.svelte";
  import {
    dataOpen,
    ddlApply,
    describeError,
    folderOf,
    formatVersion,
    objectDdl,
    type Ddl,
    type TableColumn,
    type TableShape,
  } from "./ipc";

  let {
    onedit,
    ondelete,
    onconnect,
    onquery,
    ondata,
  }: {
    onedit: (profileId: string) => void;
    ondelete: (profileId: string) => void;
    onconnect: (profileId: string) => void;
    onquery: (profileId: string, database: string, title: string) => void;
    ondata: (profileId: string, database: string, title: string, oid: number) => void;
  } = $props();

  let ddl = $state<Ddl | null>(null);
  let ddlError = $state<string | null>(null);
  let loading = $state(false);
  let copied = $state(false);

  const selected = $derived(explorer.selected);
  const node = $derived(selected?.node ?? null);
  const isServer = $derived(selected !== null && selected.node === null);
  const profile = $derived(
    selected ? (explorer.profiles.find((item) => item.id === selected.profileId) ?? null) : null,
  );
  const caps = $derived(selected ? (explorer.caps[selected.profileId] ?? null) : null);
  const look = $derived(lookOf(node?.kind ?? null));

  /** Ni las carpetas, ni las bases, ni la fila del servidor tienen un DDL propio. */
  const hasDdl = $derived(node !== null && folderOf(node.kind) === null && node.kind !== "database");

  $effect(() => {
    const current = node;
    ddl = null;
    ddlError = null;
    copied = false;

    if (!current || !hasDdl || !selected) return;

    const profileId = selected.profileId;
    let cancelled = false;
    loading = true;

    objectDdl(profileId, current)
      .then((result) => {
        if (!cancelled) ddl = result;
      })
      .catch((error) => {
        if (!cancelled) ddlError = describeError(error);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });

    // Cambiar de nodo rápido no debe dejar que una respuesta vieja pise a la nueva.
    return () => {
      cancelled = true;
    };
  });

  async function copy() {
    if (!ddl) return;
    await navigator.clipboard.writeText(ddl.sql);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  /**
   * Contra qué base abriría una consulta lo que está seleccionado. Los objetos la llevan encima;
   * la fila del servidor recién conectado usa la del perfil.
   */
  const queryTarget = $derived.by<{ database: string; title: string } | null>(() => {
    if (!selected) return null;
    if (node) return { database: node.database, title: node.label };
    if (selected.connected && profile) {
      return { database: profile.database, title: profile.name };
    }
    return null;
  });

  /**
   * Las relaciones que tienen filas para mostrar. Las vistas y las materializadas entran: se abren
   * en solo lectura, y el propio panel explica por qué.
   */
  const dataTarget = $derived.by<number | null>(() => {
    const kinds = ["table", "partitionedTable", "view", "materializedView", "foreignTable"];
    if (!node || typeof node.kind !== "string" || !kinds.includes(node.kind)) return null;
    return node.oid ?? null;
  });

  /** Solo las tablas (particionadas o no) tienen columnas que se puedan crear, cambiar o borrar. */
  const isTable = $derived(node?.kind === "table" || node?.kind === "partitionedTable");
  /** El nodo carpeta "Tablas" de un esquema, donde vive el botón para crear una tabla nueva. */
  const isTablesFolder = $derived(node !== null && folderOf(node.kind) === "tables");

  // -------------------------------------------------------------------------
  // Estructura: columnas de la tabla seleccionada
  // -------------------------------------------------------------------------

  let shape = $state<TableShape | null>(null);
  let shapeError = $state<string | null>(null);
  let shapeLoading = $state(false);

  async function loadShape() {
    if (!isTable || !node?.oid || !selected) {
      shape = null;
      return;
    }
    shapeLoading = true;
    shapeError = null;
    try {
      shape = await dataOpen(selected.profileId, node.oid, node.database);
    } catch (error) {
      shapeError = describeError(error);
    } finally {
      shapeLoading = false;
    }
  }

  $effect(() => {
    // Depender de `node` (y no llamar directo) es lo que dispara de nuevo al cambiar de tabla.
    void node;
    loadShape();
  });

  /** Busca la fila del árbol que tiene a `target` entre sus hijos, para refrescarla tras un cambio. */
  function parentOf(rows: Row[], target: Row): Row | null {
    for (const row of rows) {
      if (row.children?.includes(target)) return row;
      if (row.children) {
        const found = parentOf(row.children, target);
        if (found) return found;
      }
    }
    return null;
  }

  let newTable = $state(false);
  let columnDialog = $state<{ column: TableColumn | null } | null>(null);
  let dropTarget = $state<{ kind: "table" | "column"; label: string } | null>(null);
  let dropCascade = $state(false);
  let dropping = $state(false);
  let dropError = $state<string | null>(null);

  function afterTableCreated() {
    newTable = false;
    if (selected) explorer.reload(selected);
  }

  function afterColumnSaved() {
    columnDialog = null;
    loadShape();
  }

  async function confirmDrop() {
    if (!dropTarget || !selected || !node || !shape) return;
    dropping = true;
    dropError = null;
    try {
      if (dropTarget.kind === "table") {
        await ddlApply(
          selected.profileId,
          [{ kind: "dropTable", schema: shape.schema, name: shape.name, cascade: dropCascade }],
          node.database,
        );
        const parent = parentOf(explorer.roots, selected);
        if (parent) await explorer.reload(parent);
        explorer.selected = null;
      } else {
        await ddlApply(
          selected.profileId,
          [
            {
              kind: "dropColumn",
              schema: shape.schema,
              table: shape.name,
              column: dropTarget.label,
              cascade: dropCascade,
            },
          ],
          node.database,
        );
        await loadShape();
      }
      dropTarget = null;
      dropCascade = false;
    } catch (error) {
      dropError = describeError(error);
    } finally {
      dropping = false;
    }
  }

  const properties = $derived.by<[string, string][]>(() => {
    if (isServer && profile) {
      const rows: [string, string][] = [
        ["Servidor", `${profile.host}:${profile.port}`],
        ["Base inicial", profile.database],
        ["Usuario", profile.user],
        ["Cifrado", profile.sslMode],
      ];
      if (caps) {
        rows.push(
          ["Versión", `PostgreSQL ${formatVersion(caps.version)}`],
          ["Superusuario", caps.isSuperuser ? "sí" : "no"],
          [
            "Puede cancelar sesiones",
            caps.canSignalBackends ? "sí" : "no (falta pg_signal_backend)",
          ],
          [
            "Ve todas las estadísticas",
            caps.canReadAllStats ? "sí" : "no (falta pg_read_all_stats)",
          ],
        );
      }
      return rows;
    }

    if (!node) return [];
    const rows: [string, string][] = [["Base de datos", node.database]];
    if (node.schema) rows.push(["Esquema", node.schema]);
    if (node.oid) rows.push(["OID", String(node.oid)]);
    return rows;
  });
</script>

<div class="flex h-full flex-col">
  {#if !selected}
    <div class="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      <Icon name="schema" size={28} class="text-zinc-300 dark:text-zinc-700" />
      <p class="text-sm muted">Elegí un objeto del árbol para ver su detalle.</p>
    </div>
  {:else}
    <header class="divider-b px-5 py-4">
      <div class="flex items-center gap-2">
        <Icon name={look.icon} size={18} class={look.tone} />
        <h2 class="truncate text-base font-medium">{selected.label}</h2>
        <span class="tag tag-neutral">{kindLabel(node?.kind ?? null)}</span>

        {#if isTablesFolder && node?.schema}
          <button class="btn ml-auto shrink-0" onclick={() => (newTable = true)}>
            <Icon name="plus" size={12} />
            Tabla
          </button>
        {/if}

        {#if dataTarget !== null && queryTarget}
          <button
            class="btn shrink-0 {isTablesFolder ? '' : 'ml-auto'}"
            title={`Abre los datos de ${selected.label}`}
            onclick={() => ondata(selected.profileId, queryTarget.database, queryTarget.title, dataTarget)}
          >
            <Icon name="table" size={12} />
            Datos
          </button>
        {/if}

        {#if queryTarget}
          <button
            class="btn shrink-0 {dataTarget === null && !isTablesFolder ? 'ml-auto' : ''}"
            title={`Abre una consulta contra ${queryTarget.database}`}
            onclick={() =>
              onquery(selected.profileId, queryTarget.database, queryTarget.title)}
          >
            <Icon name="sql" size={12} />
            Consulta
          </button>
        {/if}

        {#if isTable && shape}
          <button
            class="btn shrink-0"
            onclick={() => (dropTarget = { kind: "table", label: shape!.name })}
          >
            Eliminar tabla
          </button>
        {/if}

        {#if isServer}
          <span class="flex shrink-0 gap-1.5 {queryTarget ? '' : 'ml-auto'}">
            {#if selected.connected}
              <button class="btn" onclick={() => explorer.disconnect(selected.profileId)}>
                Desconectar
              </button>
            {:else}
              <button class="btn btn-primary" onclick={() => onconnect(selected.profileId)}>
                Conectar
              </button>
            {/if}
            <button class="btn" onclick={() => onedit(selected.profileId)}>Editar</button>
            <button class="btn" onclick={() => ondelete(selected.profileId)}>Eliminar</button>
          </span>
        {/if}
      </div>

      {#if selected.comment}
        <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">{selected.comment}</p>
      {/if}
    </header>

    <div class="min-h-0 flex-1 overflow-auto p-5">
      {#if properties.length > 0}
        <dl class="mb-5 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
          {#each properties as [label, value] (label)}
            <dt class="muted">{label}</dt>
            <dd class="truncate">{value}</dd>
          {/each}
        </dl>
      {/if}

      {#if isTable}
        <div class="card mb-5 overflow-hidden">
          <div class="divider-b flex items-center gap-2 px-3 py-1.5">
            <span class="text-xs font-medium">Columnas</span>
            {#if shape}
              <button
                class="btn btn-ghost ml-auto px-2 py-0.5 text-xs"
                onclick={() => (columnDialog = { column: null })}
              >
                <Icon name="plus" size={12} />
                Columna
              </button>
            {/if}
          </div>

          {#if shapeLoading}
            <p class="px-3 py-4 text-sm muted">Leyendo columnas…</p>
          {:else if shapeError}
            <p class="px-3 py-4 text-sm text-rose-600 dark:text-rose-400">{shapeError}</p>
          {:else if shape}
            <table class="w-full text-left text-sm">
              <tbody>
                {#each shape.columns as column (column.name)}
                  <tr class="divider-t">
                    <td class="px-3 py-1.5">
                      {column.name}
                      {#if column.notNull}
                        <span class="ml-1 text-xs muted">NOT NULL</span>
                      {/if}
                    </td>
                    <td class="px-3 py-1.5 font-mono text-xs muted">{column.typeName}</td>
                    <td class="truncate px-3 py-1.5 text-xs muted">
                      {column.default ?? (column.generated ? "generada por el servidor" : "")}
                    </td>
                    <td class="px-3 py-1.5 text-right whitespace-nowrap">
                      {#if !column.generated}
                        <button
                          class="btn btn-ghost px-2 py-0.5 text-xs"
                          onclick={() => (columnDialog = { column })}
                        >
                          Editar
                        </button>
                      {/if}
                      <button
                        class="btn btn-ghost px-2 py-0.5 text-xs"
                        onclick={() => (dropTarget = { kind: "column", label: column.name })}
                      >
                        Eliminar
                      </button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
      {/if}

      {#if isServer && !selected.connected}
        <p class="text-sm muted">Conectá el servidor para explorar sus objetos.</p>
      {:else if !hasDdl && !isServer && !isTablesFolder}
        <p class="text-sm muted">Este nodo agrupa otros objetos; no tiene un DDL propio.</p>
      {:else if hasDdl}
        <div class="card overflow-hidden">
          <div class="divider-b flex items-center gap-2 px-3 py-1.5">
            <span class="text-xs font-medium">DDL</span>
            {#if ddl}
              <span class="text-xs muted">
                {ddl.source === "pgDump" ? "reconstruido con pg_dump" : "generado por PostgreSQL"}
              </span>
              <button class="btn btn-ghost ml-auto px-2 py-0.5 text-xs" onclick={copy}>
                <Icon name="copy" size={12} />
                {copied ? "Copiado" : "Copiar"}
              </button>
            {/if}
          </div>

          {#if loading}
            <p class="px-3 py-4 text-sm muted">Generando DDL…</p>
          {:else if ddlError}
            <p class="px-3 py-4 text-sm text-rose-600 dark:text-rose-400">{ddlError}</p>
          {:else if ddl}
            <pre
              class="max-h-[60vh] overflow-auto px-3 py-3 font-mono text-xs leading-relaxed
                     select-text">{ddl.sql}</pre>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

{#if newTable && selected && node?.schema}
  <TableDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    onclose={() => (newTable = false)}
    oncreated={afterTableCreated}
  />
{/if}

{#if columnDialog && selected && shape}
  <ColumnDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    column={columnDialog.column}
    onclose={() => (columnDialog = null)}
    onsaved={afterColumnSaved}
  />
{/if}

{#if dropTarget}
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
    <div class="card w-full max-w-sm p-4 shadow-xl" role="alertdialog" aria-modal="true">
      <p class="text-sm">
        {dropTarget.kind === "table"
          ? `¿Eliminar la tabla ${dropTarget.label}?`
          : `¿Eliminar la columna ${dropTarget.label}?`}
      </p>
      <label class="check mt-3 text-xs">
        <input type="checkbox" bind:checked={dropCascade} />
        CASCADE (también borra lo que depende de esto)
      </label>
      {#if dropError}
        <p class="mt-2 text-sm text-rose-600 dark:text-rose-400">{dropError}</p>
      {/if}
      <div class="mt-4 flex justify-end gap-2">
        <button
          class="btn"
          disabled={dropping}
          onclick={() => {
            dropTarget = null;
            dropCascade = false;
            dropError = null;
          }}
        >
          Cancelar
        </button>
        <button class="btn btn-primary" disabled={dropping} onclick={confirmDrop}>
          Eliminar
        </button>
      </div>
    </div>
  </div>
{/if}
