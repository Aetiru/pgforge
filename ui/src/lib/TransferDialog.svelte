<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import { t } from "./i18n.svelte";
  import Modal from "./Modal.svelte";
  import { confirmMutation } from "./access.svelte";
  import { explorer } from "./explorer.svelte";
  import {
    databaseNames,
    describeError,
    schemaNames,
    tableTransferPreview,
    type TransferMode,
    type TransferPlan,
    type TransferSpec,
  } from "./ipc";
  import { tasks } from "./tasks.svelte";

  /**
   * Copia una tabla a otro servidor.
   *
   * Solo se ofrecen servidores **conectados**, por lo mismo que en «Comparar»: conectar desde acá
   * pediría una contraseña fuera del único lugar donde se piden. Lo que se muestra en la vista
   * previa es lo que el núcleo ejecuta, no una reconstrucción.
   */
  let {
    profileId,
    database,
    schema,
    table,
    onclose,
  }: {
    profileId: string;
    database: string;
    schema: string;
    table: string;
    onclose: () => void;
  } = $props();

  const servers = $derived(explorer.profiles.filter((profile) => explorer.isConnected(profile.id)));
  const sourceName = $derived(
    explorer.profiles.find((profile) => profile.id === profileId)?.name ?? t("servidor"),
  );

  // Copia inicial tomada una sola vez, como en el resto de los formularios.
  let targetId = $state(untrack(() => profileId));
  let targetDatabase = $state(untrack(() => database));
  let targetSchema = $state(untrack(() => schema));
  let targetTable = $state(untrack(() => table));
  let mode = $state<TransferMode>("create");
  let withData = $state(true);
  let withIndexes = $state(true);
  let filter = $state("");

  let databases = $state<string[]>([]);
  let schemas = $state<string[]>([]);
  let error = $state<string | null>(null);
  let plan = $state<TransferPlan | null>(null);
  let planning = $state(false);
  let running = $state(false);

  // Derivada: el texto sigue al idioma vigente.
  const MODES: { value: TransferMode; label: string; hint: string }[] = $derived([
    { value: "create", label: t("Crear la tabla"), hint: t("La tabla no existe en el destino: se crea y se cargan las filas.") },
    { value: "append", label: t("Agregar filas"), hint: t("La tabla ya existe: se le suman las filas, sin tocar las que tiene.") },
    { value: "truncate", label: t("Reemplazar filas"), hint: t("La tabla ya existe: se vacía y se recarga, todo en una transacción.") },
  ]);

  $effect(() => {
    const id = targetId;
    let cancelled = false;
    databaseNames(id, explorer.options)
      .then((names) => {
        if (cancelled) return;
        databases = names;
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
    const [id, db] = [targetId, targetDatabase];
    if (!db) return;
    let cancelled = false;
    schemaNames(id, db)
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

  // Sobre una tabla que ya existe «solo estructura» y los índices no significan nada.
  $effect(() => {
    if (mode !== "create") {
      withData = true;
      withIndexes = false;
    }
  });

  const sameTable = $derived(
    targetId === profileId &&
      targetDatabase === database &&
      targetSchema === schema &&
      targetTable.trim() === table,
  );

  const spec = $derived<TransferSpec>({
    schema,
    table,
    targetSchema,
    targetTable: targetTable.trim(),
    mode,
    withData,
    withIndexes,
    filter: filter.trim() || null,
  });

  const ready = $derived(
    !!targetDatabase && !!targetSchema && targetTable.trim() !== "" && !sameTable,
  );

  // Cualquier cambio invalida la vista previa que ya se había pedido.
  $effect(() => {
    void [spec, targetId, targetDatabase];
    plan = null;
  });

  async function preview() {
    planning = true;
    error = null;
    try {
      plan = await tableTransferPreview(profileId, spec, database);
    } catch (problem) {
      error = describeError(problem);
    } finally {
      planning = false;
    }
  }

  async function run() {
    running = true;
    error = null;
    try {
      // La vista previa que se confirma es la del plan real: se pide si no se había mirado.
      const current = plan ?? (await tableTransferPreview(profileId, spec, database));
      const script = [...current.before, ...current.after].join(";\n");
      // La escritura es contra el destino: es su entorno el que pide la confirmación extra.
      const sure = await confirmMutation(
        targetId,
        t("Se va a copiar {source} a {target}.", {
          source: `${schema}.${table}`,
          target: `${targetSchema}.${targetTable.trim()}`,
        }),
        script,
      );
      if (!sure) return;
      await tasks.transfer({
        profileId,
        database,
        targetId,
        targetDatabase,
        spec,
      });
      onclose();
    } catch (problem) {
      error = describeError(problem);
    } finally {
      running = false;
    }
  }
</script>

<Modal title={t("Copiar tabla a otro servidor")} subtitle="{sourceName} · {database}.{schema}.{table}" size="lg" {onclose}>
  <div class="grid gap-3 sm:grid-cols-2">
    <label class="flex flex-col gap-1">
      <span class="label">{t("Servidor de destino")}</span>
      <select class="field" bind:value={targetId}>
        {#each servers as server (server.id)}
          <option value={server.id}>{server.name}</option>
        {/each}
      </select>
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">{t("Base")}</span>
      <select class="field" bind:value={targetDatabase}>
        {#each databases as name (name)}
          <option value={name}>{name}</option>
        {/each}
      </select>
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">{t("Esquema")}</span>
      <select class="field" bind:value={targetSchema}>
        {#each schemas as name (name)}
          <option value={name}>{name}</option>
        {/each}
      </select>
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">{t("Tabla")}</span>
      <input class="field font-mono" bind:value={targetTable} spellcheck="false" />
    </label>
  </div>

  <div class="seg mt-4" role="tablist">
    {#each MODES as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={mode === item.value}
        title={item.hint}
        onclick={() => (mode = item.value)}>{item.label}</button
      >
    {/each}
  </div>
  <p class="mt-1 text-xs muted">{MODES.find((item) => item.value === mode)?.hint}</p>

  {#if mode === "create"}
    <div class="mt-3 flex flex-wrap gap-x-5 gap-y-1">
      <label class="check">
        <input type="checkbox" bind:checked={withData} />
        {t("Copiar las filas")}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={withIndexes} />
        {t("Recrear los índices")}
      </label>
    </div>
  {/if}

  {#if withData}
    <label class="mt-3 flex flex-col gap-1">
      <span class="label">{t("Solo las filas que cumplan (WHERE, opcional)")}</span>
      <input
        class="field font-mono text-xs"
        bind:value={filter}
        placeholder="estado = 'activo'"
        spellcheck="false"
      />
    </label>
  {/if}

  {#if sameTable}
    <Alert tone="warn" box class="mt-3">{t("El origen y el destino son la misma tabla.")}</Alert>
  {/if}
  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if plan}
    {#each plan.warnings as warning (warning)}
      <Alert tone="warn" box class="mt-3">{warning}</Alert>
    {/each}
    <pre
      class="mt-3 max-h-56 overflow-auto rounded-md border border-zinc-200 bg-zinc-50 px-3 py-2
             font-mono text-xs whitespace-pre-wrap select-text dark:border-zinc-700
             dark:bg-zinc-700/50">{[
        ...plan.before.map((sql) => `${sql};`),
        ...(plan.copyOut
          ? [`-- ${t("origen")}:  ${plan.copyOut}`, `-- ${t("destino")}: ${plan.copyIn}`]
          : []),
        ...plan.after.map((sql) => `${sql};`),
      ].join("\n")}</pre>
  {/if}

  <p class="mt-2 text-xs muted">
    {t("Corre en segundo plano y en una sola transacción del lado del destino: si algo falla o se cancela, no queda nada a medias. Se sigue desde la vista de procesos.")}
  </p>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>{t("Cerrar")}</button>
    <button class="btn" disabled={!ready || planning || running} onclick={preview}>
      {#if planning}<span class="spinner"></span>{/if}
      {t("Ver SQL")}
    </button>
    <button class="btn btn-primary" disabled={!ready || running} onclick={run}>
      {#if running}<span class="spinner"></span>{/if}
      {t("Copiar")}
    </button>
  {/snippet}
</Modal>
