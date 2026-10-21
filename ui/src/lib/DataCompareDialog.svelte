<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { explorer } from "./explorer.svelte";
  import {
    describeError,
    schemaGraph,
    schemaNames,
    treeChildren,
    type DataSide,
    type TreeNode,
  } from "./ipc";

  /**
   * Contra qué tabla comparar los datos.
   *
   * Igual que `CompareDialog`: el origen viene dado (la tabla desde la que se abrió) y lo único que
   * se elige es el destino, entre los servidores **conectados**. La comparación lee las dos tablas
   * enteras en vivo, y conectar desde acá pediría una contraseña fuera del único lugar donde se
   * piden.
   */
  let {
    source,
    onclose,
    oncompare,
  }: {
    source: DataSide;
    onclose: () => void;
    oncompare: (source: DataSide, target: DataSide) => void;
  } = $props();

  const servers = $derived(
    explorer.profiles.filter((profile) => explorer.isConnected(profile.id)),
  );
  const sourceName = $derived(
    explorer.profiles.find((profile) => profile.id === source.id)?.name ?? "servidor",
  );

  // Copia inicial del origen: lo más probable es querer la misma tabla del otro lado. Se toma una
  // sola vez, como en el resto de los formularios.
  let targetId = $state(untrack(() => source.id));
  let targetDatabase = $state(untrack(() => source.database));
  let targetSchema = $state(untrack(() => source.schema));
  let targetTable = $state(untrack(() => source.table));

  let databases = $state<string[]>([]);
  let schemas = $state<string[]>([]);
  let tables = $state<string[]>([]);
  let error = $state<string | null>(null);

  $effect(() => {
    const id = targetId;
    let cancelled = false;
    treeChildren(id, null, explorer.options)
      .then((nodes: TreeNode[]) => {
        if (cancelled) return;
        databases = nodes.filter((node) => node.kind === "database").map((node) => node.label);
        if (!databases.includes(targetDatabase)) targetDatabase = databases[0] ?? "";
      })
      .catch((problem) => {
        if (!cancelled) error = describeError(problem);
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const [id, database] = [targetId, targetDatabase];
    if (!database) return;
    let cancelled = false;
    schemaNames(id, database)
      .then((names) => {
        if (cancelled) return;
        schemas = names;
        if (!schemas.includes(targetSchema)) targetSchema = schemas[0] ?? "";
      })
      .catch((problem) => {
        if (!cancelled) error = describeError(problem);
      });
    return () => {
      cancelled = true;
    };
  });

  // Las tablas del esquema elegido salen del mismo lectura que el diagrama (solo nombres: no se usa
  // nada más de ahí).
  $effect(() => {
    const [id, database, schema] = [targetId, targetDatabase, targetSchema];
    if (!database || !schema) return;
    let cancelled = false;
    schemaGraph(id, database, schema)
      .then((graph) => {
        if (cancelled) return;
        tables = graph.tables.map((table) => table.name);
        if (!tables.includes(targetTable)) targetTable = tables[0] ?? "";
      })
      .catch((problem) => {
        if (!cancelled) error = describeError(problem);
      });
    return () => {
      cancelled = true;
    };
  });

  const sameSide = $derived(
    targetId === source.id &&
      targetDatabase === source.database &&
      targetSchema === source.schema &&
      targetTable === source.table,
  );

  const ready = $derived(!sameSide && !!targetDatabase && !!targetSchema && !!targetTable);

  function submit() {
    if (!ready) return;
    oncompare(source, {
      id: targetId,
      database: targetDatabase,
      schema: targetSchema,
      table: targetTable,
    });
    onclose();
  }
</script>

<Modal
  title="Comparar datos"
  subtitle="{source.database}.{source.schema}.{source.table}"
  size="md"
  {onclose}
>
  <div class="grid gap-4">
    <div class="card p-3">
      <div class="label">Origen · las filas que se quieren</div>
      <div class="mt-1 text-sm select-text">
        {sourceName} · {source.database}.{source.schema}.{source.table}
      </div>
    </div>

    <div class="card p-3">
      <div class="label">Destino · la tabla que se llevaría hasta el origen</div>

      <div class="mt-2 grid gap-2">
        <label class="flex flex-col gap-1">
          <span class="label">Servidor</span>
          <select class="field" bind:value={targetId} data-autofocus>
            {#each servers as profile (profile.id)}
              <option value={profile.id}>{profile.name}</option>
            {/each}
          </select>
        </label>

        <label class="flex flex-col gap-1">
          <span class="label">Base</span>
          <select class="field" bind:value={targetDatabase}>
            {#each databases as database (database)}
              <option value={database}>{database}</option>
            {/each}
          </select>
        </label>

        <label class="flex flex-col gap-1">
          <span class="label">Esquema</span>
          <select class="field" bind:value={targetSchema}>
            {#each schemas as schema (schema)}
              <option value={schema}>{schema}</option>
            {/each}
          </select>
        </label>

        <label class="flex flex-col gap-1">
          <span class="label">Tabla</span>
          <select class="field" bind:value={targetTable}>
            {#each tables as table (table)}
              <option value={table}>{table}</option>
            {/each}
          </select>
        </label>
      </div>
    </div>

    <p class="text-xs muted">
      Las filas se emparejan por la clave primaria (o un índice único sobre columnas NOT NULL) del
      origen, y el destino tiene que tener una clave sobre las mismas columnas. Se leen las dos
      tablas enteras; nada se modifica: el resultado es un informe y un script para revisar.
    </p>

    {#if sameSide}
      <Alert tone="warn" box>Los dos lados son la misma tabla: no hay nada que comparar.</Alert>
    {/if}

    {#if error}
      <Alert tone="bad" box>{error}</Alert>
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" disabled={!ready} onclick={submit}>Comparar</button>
  {/snippet}
</Modal>
