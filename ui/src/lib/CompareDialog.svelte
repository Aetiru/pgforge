<script lang="ts">
  import { pickOpen, pickSave } from "./ipc";
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import type { CompareSource } from "./compare.svelte";
  import { explorer } from "./explorer.svelte";
  import {
    describeError,
    schemaNames,
    schemaSnapshotSave,
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
    oncompare: (source: CompareSource, target: CompareSide) => void;
  } = $props();

  /**
   * Contra qué: otro servidor conectado, o una instantánea guardada. Con una instantánea los papeles
   * se dan vuelta —el archivo es el origen y este esquema el destino—, porque el script tiene que
   * poder correr contra algo y un archivo no ejecuta nada.
   */
  let mode = $state<"server" | "file">("server");
  let snapshotPath = $state<string | null>(null);
  /** Mensaje de que la instantánea se guardó, o `null`. */
  let saved = $state<string | null>(null);
  let saving = $state(false);

  const EXTENSION = { name: "Instantánea de esquema", extensions: ["json"] };

  async function chooseSnapshot() {
    const chosen = await pickOpen({ title: "Instantánea a comparar", filters: [EXTENSION] });
    if (typeof chosen === "string") snapshotPath = chosen;
  }

  /** Guarda el esquema de origen tal como está ahora, para compararlo más adelante. */
  async function saveSnapshot() {
    const chosen = await pickSave({
      title: "Dónde guardar la instantánea",
      defaultPath: `${sourceName}-${source.database}-${source.schema}.json`,
      filters: [EXTENSION],
    });
    if (typeof chosen !== "string") return;
    saving = true;
    error = null;
    saved = null;
    try {
      await schemaSnapshotSave(source, chosen);
      saved = chosen;
    } catch (problem) {
      error = describeError(problem);
    } finally {
      saving = false;
    }
  }

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

  const ready = $derived(
    mode === "file" ? snapshotPath !== null : !sameSide && !!targetDatabase && !!targetSchema,
  );

  function submit() {
    if (!ready) return;
    if (mode === "file") oncompare(snapshotPath!, source);
    else oncompare(source, { id: targetId, database: targetDatabase, schema: targetSchema });
    onclose();
  }
</script>

<Modal title="Comparar esquemas" subtitle="{source.database}.{source.schema}" size="md" {onclose}>
  <div class="grid gap-4">
    <div class="seg" role="tablist">
      <button
        class="seg-item"
        role="tab"
        aria-selected={mode === "server"}
        onclick={() => (mode = "server")}
      >
        Contra otro esquema en vivo
      </button>
      <button
        class="seg-item"
        role="tab"
        aria-selected={mode === "file"}
        title="Qué cambió desde que se tomó una instantánea —por ejemplo, antes de un deploy—"
        onclick={() => (mode = "file")}
      >
        Contra una instantánea guardada
      </button>
    </div>

    {#if mode === "file"}
      <div class="card p-3">
        <div class="label">Origen · la instantánea, el estado que se quiere</div>
        <div class="mt-2 flex items-center gap-2">
          <span class="min-w-0 flex-1 truncate text-sm select-text" title={snapshotPath ?? ""}>
            {snapshotPath ?? "Ningún archivo elegido"}
          </span>
          <button class="btn btn-sm" data-autofocus onclick={chooseSnapshot}>Elegir…</button>
        </div>
      </div>
      <div class="card p-3">
        <div class="label">Destino · el que se llevaría hasta la instantánea</div>
        <div class="mt-1 text-sm select-text">
          {sourceName} · {source.database}.{source.schema}
        </div>
      </div>
      <p class="text-xs muted">
        El script devuelve este esquema a como estaba cuando se tomó la instantánea. Leído al revés,
        el informe dice qué cambió desde entonces.
      </p>
    {:else}
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
    {/if}

    {#if saved}
      <Alert tone="ok" box>Instantánea guardada en {saved}</Alert>
    {/if}

    {#if error}
      <Alert tone="bad" box>{error}</Alert>
    {/if}
  </div>

  {#snippet footer()}
    <!-- A la izquierda y aparte: no compara nada, guarda este esquema para compararlo después. -->
    <button
      class="btn btn-ghost mr-auto"
      disabled={saving}
      title="Guarda {source.database}.{source.schema} tal como está ahora, para compararlo más adelante"
      onclick={saveSnapshot}
    >
      {saving ? "Guardando…" : "Guardar instantánea…"}
    </button>
    <button class="btn btn-ghost" onclick={onclose}>Cancelar</button>
    <button class="btn btn-primary" disabled={!ready} onclick={submit}>Comparar</button>
  {/snippet}
</Modal>
