<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { explorer } from "./explorer.svelte";
  import {
    describeError,
    schemaNames,
    treeChildren,
    type CompareSide,
    type TreeNode,
  } from "./ipc";

  /**
   * Contra qué comparar.
   *
   * El origen viene dado —es el esquema desde el que se abrió— y se muestra sin poder cambiarlo: si
   * uno quiso comparar otro, lo abre desde ese otro. Lo que se elige acá es el destino, y por eso es
   * lo único que tiene selectores.
   *
   * Solo se ofrecen los servidores **conectados**: la comparación lee en vivo de los dos lados, y
   * conectar desde este diálogo pediría una contraseña fuera del único lugar donde se piden.
   */
  let {
    source,
    onclose,
    oncompare,
  }: {
    source: CompareSide;
    onclose: () => void;
    oncompare: (source: CompareSide, target: CompareSide) => void;
  } = $props();

  const servers = $derived(
    explorer.profiles.filter((profile) => explorer.isConnected(profile.id)),
  );
  const sourceName = $derived(
    explorer.profiles.find((profile) => profile.id === source.id)?.name ?? "servidor",
  );

  // Copia inicial del origen: es lo más probable que se quiera del otro lado —el mismo esquema, la
  // misma base— y desde ahí se cambia lo que haga falta. Se toma una sola vez, como en el resto de
  // los formularios: si `source` cambiara, este diálogo ya se cerró.
  let targetId = $state(untrack(() => source.id));
  let targetDatabase = $state(untrack(() => source.database));
  let targetSchema = $state(untrack(() => source.schema));

  let databases = $state<string[]>([]);
  let schemas = $state<string[]>([]);
  let error = $state<string | null>(null);

  // Las bases del servidor elegido salen del mismo lugar que la raíz del árbol: con `parent` en
  // `null`, `treeChildren` devuelve las bases —más la carpeta de roles, que acá no va—.
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
        // El mismo nombre de esquema de los dos lados es lo normal; si no está, se ofrece el primero
        // en vez de dejar el selector apuntando a algo que no existe.
        if (!schemas.includes(targetSchema)) targetSchema = schemas[0] ?? "";
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
      targetSchema === source.schema,
  );

  function submit() {
    if (sameSide || !targetDatabase || !targetSchema) return;
    oncompare(source, { id: targetId, database: targetDatabase, schema: targetSchema });
    onclose();
  }
</script>

<Modal title="Comparar esquemas" subtitle="{source.database}.{source.schema}" size="md" {onclose}>
  <div class="grid gap-4">
    <div class="card p-3">
      <div class="label">Origen · el estado que se quiere</div>
      <div class="mt-1 text-sm select-text">
        {sourceName} · {source.database}.{source.schema}
      </div>
    </div>

    <div class="card p-3">
      <div class="label">Destino · el que se llevaría hasta el origen</div>

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
      </div>
    </div>

    {#if servers.length < 2}
      <Alert tone="warn" box>
        Hay un solo servidor conectado. Se puede comparar contra otro esquema del mismo, o conectar
        el otro servidor y volver a abrir esta ventana.
      </Alert>
    {/if}

    {#if sameSide}
      <Alert tone="warn" box>Los dos lados son el mismo esquema: no hay nada que comparar.</Alert>
    {/if}

    {#if error}
      <Alert tone="bad" box>{error}</Alert>
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancelar</button>
    <button
      class="btn btn-primary"
      disabled={sameSide || !targetDatabase || !targetSchema}
      onclick={submit}
    >
      Comparar
    </button>
  {/snippet}
</Modal>
