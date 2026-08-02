<script lang="ts">
  import ColumnDialog from "./ColumnDialog.svelte";
  import ConstraintDialog from "./ConstraintDialog.svelte";
  import FunctionDialog from "./FunctionDialog.svelte";
  import Icon from "./Icon.svelte";
  import IndexDialog from "./IndexDialog.svelte";
  import RoleDialog from "./RoleDialog.svelte";
  import TableDialog from "./TableDialog.svelte";
  import TriggerDialog from "./TriggerDialog.svelte";
  import ViewDialog from "./ViewDialog.svelte";
  import { kindLabel, lookOf } from "./badges";
  import { explorer, type Row } from "./explorer.svelte";
  import {
    dataOpen,
    ddlApply,
    describeError,
    folderOf,
    formatVersion,
    functionArgs,
    functionDrop,
    indexDrop,
    objectDdl,
    roleApply,
    roleInfo,
    tableConstraints,
    tableIndexes,
    tableTriggers,
    triggerApply,
    viewApply,
    type ConstraintInfo,
    type Ddl,
    type IndexInfo,
    type RoleInfo,
    type TableColumn,
    type TableShape,
    type TriggerInfo,
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

  /**
   * Re-lee el DDL del nodo actual sin esperar a que cambie de nodo. Hace falta después de editar
   * una vista: el nodo sigue siendo el mismo, así que el efecto de arriba no se vuelve a disparar
   * solo.
   */
  async function refreshDdl() {
    if (!node || !hasDdl || !selected) return;
    loading = true;
    ddlError = null;
    try {
      ddl = await objectDdl(selected.profileId, node);
    } catch (error) {
      ddlError = describeError(error);
    } finally {
      loading = false;
    }
  }

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

  const isView = $derived(node?.kind === "view");
  const isMaterializedView = $derived(node?.kind === "materializedView");
  const isViewsFolder = $derived(node !== null && folderOf(node.kind) === "views");
  const isMatViewsFolder = $derived(node !== null && folderOf(node.kind) === "materializedViews");

  const isFunction = $derived(node?.kind === "function");
  const isProcedure = $derived(node?.kind === "procedure");
  const isFunctionsFolder = $derived(node !== null && folderOf(node.kind) === "functions");
  const isProceduresFolder = $derived(node !== null && folderOf(node.kind) === "procedures");

  const isRole = $derived(node?.kind === "role");
  /** La única carpeta que no cuelga de una base: es hermana de todas ellas en la raíz. */
  const isRolesFolder = $derived(node !== null && folderOf(node.kind) === "roles");

  /** Cualquiera de los botones "+" de carpeta: solo puede haber uno a la vez para un mismo nodo. */
  const hasCreateFolderButton = $derived(
    isTablesFolder ||
      isViewsFolder ||
      isMatViewsFolder ||
      isFunctionsFolder ||
      isProceduresFolder ||
      isRolesFolder,
  );

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

  let indexes = $state<IndexInfo[] | null>(null);
  let indexesError = $state<string | null>(null);
  let indexesLoading = $state(false);

  async function loadIndexes() {
    if (!isTable || !node?.oid || !selected) {
      indexes = null;
      return;
    }
    indexesLoading = true;
    indexesError = null;
    try {
      indexes = await tableIndexes(selected.profileId, node.oid, node.database);
    } catch (error) {
      indexesError = describeError(error);
    } finally {
      indexesLoading = false;
    }
  }

  let constraints = $state<ConstraintInfo[] | null>(null);
  let constraintsError = $state<string | null>(null);
  let constraintsLoading = $state(false);

  async function loadConstraints() {
    if (!isTable || !node?.oid || !selected) {
      constraints = null;
      return;
    }
    constraintsLoading = true;
    constraintsError = null;
    try {
      constraints = await tableConstraints(selected.profileId, node.oid, node.database);
    } catch (error) {
      constraintsError = describeError(error);
    } finally {
      constraintsLoading = false;
    }
  }

  let triggers = $state<TriggerInfo[] | null>(null);
  let triggersError = $state<string | null>(null);
  let triggersLoading = $state(false);

  async function loadTriggers() {
    if (!isTable || !node?.oid || !selected) {
      triggers = null;
      return;
    }
    triggersLoading = true;
    triggersError = null;
    try {
      triggers = await tableTriggers(selected.profileId, node.oid, node.database);
    } catch (error) {
      triggersError = describeError(error);
    } finally {
      triggersLoading = false;
    }
  }

  $effect(() => {
    // Depender de `node` (y no llamar directo) es lo que dispara de nuevo al cambiar de tabla.
    void node;
    loadShape();
    loadIndexes();
    loadConstraints();
    loadTriggers();
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
  let newIndex = $state(false);
  let newConstraint = $state(false);
  let columnDialog = $state<{ column: TableColumn | null } | null>(null);
  let viewDialog = $state<{
    materialized: boolean;
    existing: { oid: number; name: string } | null;
  } | null>(null);
  let refreshTarget = $state<{ schema: string; name: string } | null>(null);
  let refreshConcurrently = $state(false);
  let refreshing = $state(false);
  let refreshError = $state<string | null>(null);
  let functionDialog = $state<{ sql: string; isEdit: boolean } | null>(null);
  let triggerDialog = $state<{ existing: TriggerInfo | null } | null>(null);
  let roleDialog = $state<{ existing: RoleInfo | null } | null>(null);
  let dropTarget = $state<
    | {
        kind:
          | "table"
          | "column"
          | "index"
          | "constraint"
          | "view"
          | "materializedView"
          | "trigger"
          | "role";
        label: string;
      }
    | { kind: "function"; schema: string; name: string; args: string; procedure: boolean }
    | null
  >(null);
  let dropCascade = $state(false);
  /** Solo se usa para índices: CASCADE y CONCURRENTLY son mutuamente excluyentes en Postgres. */
  let dropConcurrently = $state(false);
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

  function afterIndexCreated() {
    newIndex = false;
    loadIndexes();
  }

  function afterTriggerSaved() {
    triggerDialog = null;
    loadTriggers();
  }

  /** Trae el rol tal como ya existe antes de abrir la edición: acá no hay nada precargado, a
   * diferencia de una columna, que ya viene con `shape`. */
  async function openEditRole() {
    if (!selected || !node?.oid) return;
    try {
      const info = await roleInfo(selected.profileId, node.oid, node.database);
      roleDialog = { existing: info };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  function afterRoleSaved() {
    const wasCreate = roleDialog !== null && roleDialog.existing === null;
    roleDialog = null;
    if (!selected) return;
    if (wasCreate) {
      explorer.reload(selected);
    } else {
      // Renombrar cambia lo que el árbol muestra: se recarga la carpeta "Roles" y se limpia la
      // selección, porque la fila que estaba elegida ya no coincide con el rol que quedó.
      const parent = parentOf(explorer.roots, selected);
      if (parent) explorer.reload(parent);
      explorer.selected = null;
    }
  }

  const TIMING_LABEL: Record<TriggerInfo["timing"], string> = {
    before: "BEFORE",
    after: "AFTER",
    insteadOf: "INSTEAD OF",
  };
  const EVENT_LABEL: Record<TriggerInfo["events"][number], string> = {
    insert: "INSERT",
    update: "UPDATE",
    delete: "DELETE",
    truncate: "TRUNCATE",
  };

  function triggerSummary(t: TriggerInfo): string {
    const events = t.events.map((event) => EVENT_LABEL[event]).join(" OR ");
    return `${TIMING_LABEL[t.timing]} ${events} · ${t.level === "row" ? "ROW" : "STATEMENT"}`;
  }

  function afterConstraintCreated() {
    newConstraint = false;
    loadConstraints();
    // Agregar una primary key o un unique puede cambiar si la grilla de datos de la tabla es
    // editable: se relee la forma para que ese estado no quede desactualizado en el panel.
    loadShape();
  }

  function closeDropDialog() {
    dropTarget = null;
    dropCascade = false;
    dropConcurrently = false;
    dropError = null;
  }

  /** Un punto de partida mínimo: mejor que una pantalla en blanco, sin fingir saber qué necesita. */
  function functionSkeleton(schema: string, procedure: boolean): string {
    return procedure
      ? `CREATE PROCEDURE ${schema}.nombre()\nLANGUAGE plpgsql\nAS $$\nBEGIN\nEND;\n$$;`
      : `CREATE FUNCTION ${schema}.nombre()\nRETURNS void\nLANGUAGE plpgsql\nAS $$\nBEGIN\nEND;\n$$;`;
  }

  function afterFunctionSaved() {
    const wasCreate = functionDialog !== null && !functionDialog.isEdit;
    functionDialog = null;
    if (wasCreate) {
      if (selected) explorer.reload(selected);
    } else {
      refreshDdl();
    }
  }

  /** Busca la firma completa antes de confirmar: sin ella no se puede armar el DROP FUNCTION. */
  async function askDropFunction() {
    if (!selected || !node?.oid || !node.schema) return;
    try {
      const args = await functionArgs(selected.profileId, node.oid, node.database);
      dropTarget = { kind: "function", schema: node.schema, name: node.label, args, procedure: isProcedure };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  /** Recarga el nodo carpeta que pasó a tener un objeto nuevo, o refresca el DDL tras editarlo. */
  function afterViewSaved() {
    const wasCreate = viewDialog !== null && viewDialog.existing === null;
    viewDialog = null;
    if (wasCreate) {
      if (selected) explorer.reload(selected);
    } else {
      refreshDdl();
    }
  }

  async function confirmRefresh() {
    if (!refreshTarget || !selected) return;
    refreshing = true;
    refreshError = null;
    try {
      await viewApply(
        selected.profileId,
        [
          {
            kind: "refreshMaterializedView",
            schema: refreshTarget.schema,
            name: refreshTarget.name,
            concurrently: refreshConcurrently,
          },
        ],
        node?.database,
      );
      refreshTarget = null;
      refreshConcurrently = false;
    } catch (error) {
      refreshError = describeError(error);
    } finally {
      refreshing = false;
    }
  }

  async function confirmDrop() {
    if (!dropTarget || !selected || !node) return;
    dropping = true;
    dropError = null;
    try {
      switch (dropTarget.kind) {
        case "table":
          await ddlApply(
            selected.profileId,
            [{ kind: "dropTable", schema: shape!.schema, name: shape!.name, cascade: dropCascade }],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "column":
          await ddlApply(
            selected.profileId,
            [
              {
                kind: "dropColumn",
                schema: shape!.schema,
                table: shape!.name,
                column: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          await loadShape();
          break;
        case "index":
          await indexDrop(
            selected.profileId,
            shape!.schema,
            dropTarget.label,
            dropCascade,
            dropConcurrently,
            node.database,
          );
          await loadIndexes();
          break;
        case "constraint":
          await ddlApply(
            selected.profileId,
            [
              {
                kind: "dropConstraint",
                schema: shape!.schema,
                table: shape!.name,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          await loadConstraints();
          await loadShape();
          break;
        case "trigger":
          await triggerApply(
            selected.profileId,
            [
              {
                kind: "dropTrigger",
                schema: shape!.schema,
                table: shape!.name,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          await loadTriggers();
          break;
        case "view":
          await viewApply(
            selected.profileId,
            [{ kind: "dropView", schema: node.schema!, name: dropTarget.label, cascade: dropCascade }],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "materializedView":
          await viewApply(
            selected.profileId,
            [
              {
                kind: "dropMaterializedView",
                schema: node.schema!,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "function":
          await functionDrop(
            selected.profileId,
            dropTarget.schema,
            dropTarget.name,
            dropTarget.args,
            dropTarget.procedure,
            dropCascade,
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "role":
          await roleApply(selected.profileId, [{ kind: "dropRole", name: dropTarget.label }], node.database);
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
      }
      closeDropDialog();
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

        {#if isViewsFolder && node?.schema}
          <button
            class="btn ml-auto shrink-0"
            onclick={() => (viewDialog = { materialized: false, existing: null })}
          >
            <Icon name="plus" size={12} />
            Vista
          </button>
        {/if}

        {#if isMatViewsFolder && node?.schema}
          <button
            class="btn ml-auto shrink-0"
            onclick={() => (viewDialog = { materialized: true, existing: null })}
          >
            <Icon name="plus" size={12} />
            Vista materializada
          </button>
        {/if}

        {#if isFunctionsFolder && node?.schema}
          <button
            class="btn ml-auto shrink-0"
            onclick={() => (functionDialog = { sql: functionSkeleton(node!.schema!, false), isEdit: false })}
          >
            <Icon name="plus" size={12} />
            Función
          </button>
        {/if}

        {#if isProceduresFolder && node?.schema}
          <button
            class="btn ml-auto shrink-0"
            onclick={() => (functionDialog = { sql: functionSkeleton(node!.schema!, true), isEdit: false })}
          >
            <Icon name="plus" size={12} />
            Procedimiento
          </button>
        {/if}

        {#if isRolesFolder}
          <button class="btn ml-auto shrink-0" onclick={() => (roleDialog = { existing: null })}>
            <Icon name="plus" size={12} />
            Rol
          </button>
        {/if}

        {#if dataTarget !== null && queryTarget}
          <button
            class="btn shrink-0 {hasCreateFolderButton ? '' : 'ml-auto'}"
            title={`Abre los datos de ${selected.label}`}
            onclick={() => ondata(selected.profileId, queryTarget.database, queryTarget.title, dataTarget)}
          >
            <Icon name="table" size={12} />
            Datos
          </button>
        {/if}

        {#if queryTarget}
          <button
            class="btn shrink-0 {dataTarget === null && !hasCreateFolderButton ? 'ml-auto' : ''}"
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

        {#if isView && node}
          <button
            class="btn shrink-0"
            onclick={() => (viewDialog = { materialized: false, existing: { oid: node!.oid!, name: node!.label } })}
          >
            Editar
          </button>
          <button
            class="btn shrink-0"
            onclick={() => (dropTarget = { kind: "view", label: node!.label })}
          >
            Eliminar vista
          </button>
        {/if}

        {#if isMaterializedView && node}
          <button
            class="btn shrink-0"
            onclick={() => (viewDialog = { materialized: true, existing: { oid: node!.oid!, name: node!.label } })}
          >
            Editar
          </button>
          <button
            class="btn shrink-0"
            onclick={() => (refreshTarget = { schema: node!.schema!, name: node!.label })}
          >
            Refrescar
          </button>
          <button
            class="btn shrink-0"
            onclick={() => (dropTarget = { kind: "materializedView", label: node!.label })}
          >
            Eliminar vista materializada
          </button>
        {/if}

        {#if (isFunction || isProcedure) && ddl}
          <button
            class="btn shrink-0"
            onclick={() => (functionDialog = { sql: ddl!.sql, isEdit: true })}
          >
            Editar
          </button>
          <button class="btn shrink-0" onclick={askDropFunction}>
            Eliminar {isProcedure ? "procedimiento" : "función"}
          </button>
        {/if}

        {#if isRole}
          <button class="btn shrink-0" onclick={openEditRole}>Editar</button>
          <button
            class="btn shrink-0"
            onclick={() => (dropTarget = { kind: "role", label: selected.label })}
          >
            Eliminar rol
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

        <div class="card mb-5 overflow-hidden">
          <div class="divider-b flex items-center gap-2 px-3 py-1.5">
            <span class="text-xs font-medium">Índices</span>
            {#if shape}
              <button
                class="btn btn-ghost ml-auto px-2 py-0.5 text-xs"
                onclick={() => (newIndex = true)}
              >
                <Icon name="plus" size={12} />
                Índice
              </button>
            {/if}
          </div>

          {#if indexesLoading}
            <p class="px-3 py-4 text-sm muted">Leyendo índices…</p>
          {:else if indexesError}
            <p class="px-3 py-4 text-sm text-rose-600 dark:text-rose-400">{indexesError}</p>
          {:else if indexes && indexes.length === 0}
            <p class="px-3 py-4 text-sm muted">No tiene índices propios.</p>
          {:else if indexes}
            <table class="w-full text-left text-sm">
              <tbody>
                {#each indexes as index (index.oid)}
                  <tr class="divider-t">
                    <td class="px-3 py-1.5">
                      {index.name}
                      {#if index.primary}
                        <span class="ml-1 text-xs muted">primario</span>
                      {:else if index.unique}
                        <span class="ml-1 text-xs muted">único</span>
                      {/if}
                      {#if !index.valid}
                        <span class="ml-1 text-xs text-rose-600 dark:text-rose-400">inválido</span>
                      {/if}
                    </td>
                    <td class="truncate px-3 py-1.5 font-mono text-xs muted" title={index.definition}>
                      {index.definition}
                    </td>
                    <td class="px-3 py-1.5 text-right whitespace-nowrap">
                      <button
                        class="btn btn-ghost px-2 py-0.5 text-xs"
                        onclick={() => (dropTarget = { kind: "index", label: index.name })}
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

        <div class="card mb-5 overflow-hidden">
          <div class="divider-b flex items-center gap-2 px-3 py-1.5">
            <span class="text-xs font-medium">Restricciones</span>
            {#if shape}
              <button
                class="btn btn-ghost ml-auto px-2 py-0.5 text-xs"
                onclick={() => (newConstraint = true)}
              >
                <Icon name="plus" size={12} />
                Restricción
              </button>
            {/if}
          </div>

          {#if constraintsLoading}
            <p class="px-3 py-4 text-sm muted">Leyendo restricciones…</p>
          {:else if constraintsError}
            <p class="px-3 py-4 text-sm text-rose-600 dark:text-rose-400">{constraintsError}</p>
          {:else if constraints && constraints.length === 0}
            <p class="px-3 py-4 text-sm muted">No tiene restricciones propias.</p>
          {:else if constraints}
            <table class="w-full text-left text-sm">
              <tbody>
                {#each constraints as constraint (constraint.oid)}
                  <tr class="divider-t">
                    <td class="px-3 py-1.5">
                      {constraint.name}
                      <span class="ml-1 text-xs muted">{constraint.kind}</span>
                    </td>
                    <td
                      class="truncate px-3 py-1.5 font-mono text-xs muted"
                      title={constraint.definition}
                    >
                      {constraint.definition}
                    </td>
                    <td class="px-3 py-1.5 text-right whitespace-nowrap">
                      <button
                        class="btn btn-ghost px-2 py-0.5 text-xs"
                        onclick={() => (dropTarget = { kind: "constraint", label: constraint.name })}
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

        <div class="card mb-5 overflow-hidden">
          <div class="divider-b flex items-center gap-2 px-3 py-1.5">
            <span class="text-xs font-medium">Triggers</span>
            {#if shape}
              <button
                class="btn btn-ghost ml-auto px-2 py-0.5 text-xs"
                onclick={() => (triggerDialog = { existing: null })}
              >
                <Icon name="plus" size={12} />
                Trigger
              </button>
            {/if}
          </div>

          {#if triggersLoading}
            <p class="px-3 py-4 text-sm muted">Leyendo triggers…</p>
          {:else if triggersError}
            <p class="px-3 py-4 text-sm text-rose-600 dark:text-rose-400">{triggersError}</p>
          {:else if triggers && triggers.length === 0}
            <p class="px-3 py-4 text-sm muted">No tiene triggers propios.</p>
          {:else if triggers}
            <table class="w-full text-left text-sm">
              <tbody>
                {#each triggers as trigger (trigger.oid)}
                  <tr class="divider-t">
                    <td class="px-3 py-1.5">
                      {trigger.name}
                      <span class="ml-1 text-xs muted">{triggerSummary(trigger)}</span>
                    </td>
                    <td class="truncate px-3 py-1.5 font-mono text-xs muted">
                      {trigger.functionSchema}.{trigger.functionName}()
                    </td>
                    <td class="px-3 py-1.5 text-right whitespace-nowrap">
                      <button
                        class="btn btn-ghost px-2 py-0.5 text-xs"
                        onclick={() => (triggerDialog = { existing: trigger })}
                      >
                        Editar
                      </button>
                      <button
                        class="btn btn-ghost px-2 py-0.5 text-xs"
                        onclick={() => (dropTarget = { kind: "trigger", label: trigger.name })}
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

{#if newIndex && selected && shape}
  <IndexDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    columns={shape.columns}
    onclose={() => (newIndex = false)}
    oncreated={afterIndexCreated}
  />
{/if}

{#if newConstraint && selected && shape}
  <ConstraintDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    columns={shape.columns}
    onclose={() => (newConstraint = false)}
    oncreated={afterConstraintCreated}
  />
{/if}

{#if triggerDialog && selected && shape}
  <TriggerDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    existing={triggerDialog.existing}
    onclose={() => (triggerDialog = null)}
    onsaved={afterTriggerSaved}
  />
{/if}

{#if viewDialog && selected && node?.schema}
  <ViewDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    materialized={viewDialog.materialized}
    existing={viewDialog.existing}
    onclose={() => (viewDialog = null)}
    onsaved={afterViewSaved}
  />
{/if}

{#if functionDialog && selected}
  <FunctionDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    sql={functionDialog.sql}
    onclose={() => (functionDialog = null)}
    onsaved={afterFunctionSaved}
  />
{/if}

{#if roleDialog && selected}
  <RoleDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    existing={roleDialog.existing}
    onclose={() => (roleDialog = null)}
    onsaved={afterRoleSaved}
  />
{/if}

{#if refreshTarget}
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
    <div class="card w-full max-w-sm p-4 shadow-xl" role="alertdialog" aria-modal="true">
      <p class="text-sm">¿Refrescar {refreshTarget.name}?</p>
      <label class="check mt-3 text-xs">
        <input type="checkbox" bind:checked={refreshConcurrently} />
        CONCURRENTLY (no bloquea a los lectores; necesita un índice único)
      </label>
      {#if refreshError}
        <p class="mt-2 text-sm text-rose-600 dark:text-rose-400">{refreshError}</p>
      {/if}
      <div class="mt-4 flex justify-end gap-2">
        <button
          class="btn"
          disabled={refreshing}
          onclick={() => {
            refreshTarget = null;
            refreshConcurrently = false;
            refreshError = null;
          }}
        >
          Cancelar
        </button>
        <button class="btn btn-primary" disabled={refreshing} onclick={confirmRefresh}>
          Refrescar
        </button>
      </div>
    </div>
  </div>
{/if}

{#if dropTarget}
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
    <div class="card w-full max-w-sm p-4 shadow-xl" role="alertdialog" aria-modal="true">
      <p class="text-sm">
        {#if dropTarget.kind === "table"}
          ¿Eliminar la tabla {dropTarget.label}?
        {:else if dropTarget.kind === "column"}
          ¿Eliminar la columna {dropTarget.label}?
        {:else if dropTarget.kind === "index"}
          ¿Eliminar el índice {dropTarget.label}?
        {:else if dropTarget.kind === "constraint"}
          ¿Eliminar la restricción {dropTarget.label}?
        {:else if dropTarget.kind === "trigger"}
          ¿Eliminar el trigger {dropTarget.label}?
        {:else if dropTarget.kind === "view"}
          ¿Eliminar la vista {dropTarget.label}?
        {:else if dropTarget.kind === "materializedView"}
          ¿Eliminar la vista materializada {dropTarget.label}?
        {:else if dropTarget.kind === "function"}
          ¿Eliminar {dropTarget.procedure ? "el procedimiento" : "la función"}
          {dropTarget.name}({dropTarget.args})?
        {:else if dropTarget.kind === "role"}
          ¿Eliminar el rol {dropTarget.label}?
        {/if}
      </p>
      {#if dropTarget.kind === "index"}
        <label class="check mt-3 text-xs">
          <input type="checkbox" bind:checked={dropConcurrently} />
          CONCURRENTLY (no bloquea la tabla; no se puede combinar con CASCADE)
        </label>
      {:else if dropTarget.kind !== "role"}
        <label class="check mt-3 text-xs">
          <input type="checkbox" bind:checked={dropCascade} />
          CASCADE (también borra lo que depende de esto)
        </label>
      {/if}
      {#if dropError}
        <p class="mt-2 text-sm text-rose-600 dark:text-rose-400">{dropError}</p>
      {/if}
      <div class="mt-4 flex justify-end gap-2">
        <button class="btn" disabled={dropping} onclick={closeDropDialog}>Cancelar</button>
        <button class="btn btn-primary" disabled={dropping} onclick={confirmDrop}>
          Eliminar
        </button>
      </div>
    </div>
  </div>
{/if}
