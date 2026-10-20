<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import Sql from "./Sql.svelte";
  import { confirmMutation, readOnlyReason } from "./access.svelte";
  import {
    STATE_HINT,
    STATE_LABEL,
    STATE_TONE,
    applyBlocker,
    descriptionText,
    revertBlocker,
    shortSum,
    versionLabel,
  } from "./migration-view";
  import type { MigrationsTab } from "./migrations.svelte";
  import { tasks } from "./tasks.svelte";
  import {
    describeError,
    migrationPreview,
    type MigrationDirection,
    type MigrationItem,
    type MigrationPlan,
  } from "./ipc";

  /**
   * Las migraciones de una base: cuáles faltan, cuáles se aplicaron y con qué resultado.
   *
   * Lo que se aplica pasa por el camino de siempre: vista previa con el SQL exacto, confirmación
   * extra en producción y tarea en segundo plano. El orden (la primera pendiente, deshacer solo la
   * última) lo hace cumplir el núcleo; acá solo se apagan los botones que iba a rechazar.
   */
  let { tab }: { tab: MigrationsTab } = $props();

  let reviewing = $state<{ item: MigrationItem; plan: MigrationPlan } | null>(null);
  let actionError = $state<string | null>(null);
  let busy = $state(false);
  let showLog = $state(false);

  const items = $derived(tab.overview?.report.items ?? []);
  const missing = $derived(tab.overview?.report.missing ?? []);
  const orphans = $derived(tab.overview?.orphanDowns ?? []);
  const records = $derived([...(tab.overview?.records ?? [])].reverse());
  const readOnly = $derived(readOnlyReason(tab.profileId));
  const pending = $derived(items.filter((item) => item.state === "pending" || item.state === "failed"));

  $effect(() => {
    // Se lee al abrir la pestaña si ya tiene carpeta (por ejemplo, tras «Guardar como migración»).
    if (tab.folder && tab.overview === null && !tab.loading && tab.error === null) void tab.load();
  });

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleString("es", {
      day: "2-digit",
      month: "2-digit",
      year: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  }

  async function review(item: MigrationItem, direction: MigrationDirection) {
    if (!tab.folder) return;
    actionError = null;
    busy = true;
    try {
      const plan = await migrationPreview(
        tab.profileId,
        tab.database,
        tab.folder,
        item.version,
        direction,
      );
      reviewing = { item, plan };
    } catch (error) {
      actionError = describeError(error);
    } finally {
      busy = false;
    }
  }

  async function confirm() {
    if (!reviewing || !tab.folder) return;
    const { plan } = reviewing;
    const label = `${plan.direction === "up" ? "aplicar" : "deshacer"} ${versionLabel(plan.version)}`;
    // La confirmación de producción es la de siempre y se resuelve por promesa, sin anidar otro modal.
    if (!(await confirmMutation(tab.profileId, label))) return;

    busy = true;
    try {
      await tasks.migration({
        profileId: tab.profileId,
        database: tab.database,
        folder: tab.folder,
        version: plan.version,
        direction: plan.direction,
        checksum: plan.checksum,
        onDone: () => void tab.load(),
      });
      reviewing = null;
    } catch (error) {
      // El archivo pudo cambiar desde la vista previa, o el orden ya no ser el mismo: se explica
      // acá, con el diálogo abierto.
      actionError = describeError(error);
      reviewing = null;
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <div class="toolbar">
    <Icon name="sql" size={13} class="muted" />
    <span class="text-sm font-medium">Migraciones</span>
    <span class="text-xs muted">{tab.database}</span>
    <span class="ml-auto"></span>
    {#if tab.folder}
      <span class="max-w-[40%] truncate font-mono text-xs muted select-text" title={tab.folder}>
        {tab.folder}
      </span>
    {/if}
    <button class="btn btn-sm" onclick={() => tab.chooseFolder()}>
      <Icon name="folder" size={11} />
      {tab.folder ? "Cambiar carpeta…" : "Elegir carpeta…"}
    </button>
    <button
      class="btn btn-sm btn-ghost btn-icon"
      aria-label="Releer la carpeta y el registro"
      title="Releer la carpeta y el registro"
      disabled={!tab.folder || tab.loading}
      onclick={() => tab.load()}
    >
      <Icon name="refresh" size={12} />
    </button>
  </div>

  {#if !tab.folder}
    <Empty
      icon="folder"
      title="Elegí la carpeta de migraciones"
      hint="Son archivos V001__descripcion.sql, con su U001__… opcional para deshacer. Se guardan desde el informe de «Comparar esquemas», o se escriben a mano. Lo aplicado queda anotado en esta máquina, por servidor y base."
    >
      <button class="btn btn-primary" onclick={() => tab.chooseFolder()}>Elegir carpeta…</button>
    </Empty>
  {:else}
    <div class="min-h-0 flex-1 overflow-auto p-3">
      {#if tab.error}
        <Alert tone="bad" box>{tab.error}</Alert>
      {/if}
      {#if actionError}
        <Alert tone="bad" box class="mb-3">{actionError}</Alert>
      {/if}
      {#if readOnly}
        <Alert tone="warn" box class="mb-3">{readOnly}: se pueden ver, no aplicar.</Alert>
      {/if}

      {#if tab.loading && tab.overview === null}
        <p class="p-4 text-sm muted"><span class="spinner"></span> Leyendo la carpeta…</p>
      {:else if tab.overview}
        {#if missing.length > 0}
          <Alert tone="warn" box class="mb-3">
            El registro dice que se aplicó
            {missing.map((m) => versionLabel(m.version)).join(", ")}, pero ya no está en la carpeta.
          </Alert>
        {/if}
        {#if orphans.length > 0}
          <Alert tone="warn" box class="mb-3">
            Hay scripts de reversa sin su migración: {orphans.join(", ")}.
          </Alert>
        {/if}

        {#if items.length === 0}
          <Empty
            icon="sql"
            title="La carpeta no tiene migraciones"
            hint="Guardá el script de una comparación desde «Comparar esquemas» → «Guardar como migración»."
          />
        {:else}
          <p class="mb-2 text-xs muted">
            {pending.length === 0
              ? "No falta aplicar nada."
              : `${pending.length} pendiente${pending.length === 1 ? "" : "s"} de ${items.length}.`}
          </p>
          <table class="list-table">
            <thead>
              <tr>
                <th>Versión</th>
                <th>Descripción</th>
                <th>Estado</th>
                <th>Aplicada</th>
                <th class="text-right">Acciones</th>
              </tr>
            </thead>
            <tbody>
              {#each items as item (item.version)}
                {@const applyBlock = readOnly ?? applyBlocker(items, item)}
                {@const revertBlock = readOnly ?? revertBlocker(items, item)}
                <tr>
                  <td class="font-mono text-xs whitespace-nowrap">{versionLabel(item.version)}</td>
                  <td>
                    {descriptionText(item.description)}
                    {#if item.outOfOrder}
                      <span
                        class="tag tag-warn ml-1"
                        title="Hay migraciones con número mayor ya aplicadas en este servidor"
                      >
                        fuera de orden
                      </span>
                    {/if}
                    {#if item.error}
                      <p class="mt-0.5 text-xs text-rose-600 select-text dark:text-rose-400">
                        {item.error}
                      </p>
                    {/if}
                  </td>
                  <td>
                    <span class="tag {STATE_TONE[item.state]}" title={STATE_HINT[item.state]}>
                      {STATE_LABEL[item.state]}
                    </span>
                  </td>
                  <td class="text-xs whitespace-nowrap tabular-nums muted">
                    {#if item.appliedAt !== null}
                      <span title="Huella {item.appliedChecksum ?? ''}">{when(item.appliedAt)}</span>
                    {:else}
                      —
                    {/if}
                  </td>
                  <td class="text-right whitespace-nowrap">
                    {#if item.state === "pending" || item.state === "failed"}
                      <button
                        class="btn btn-sm btn-primary"
                        disabled={busy || applyBlock !== null}
                        title={applyBlock ?? "Ver el SQL y aplicarla"}
                        onclick={() => review(item, "up")}
                      >
                        {item.state === "failed" ? "Reintentar…" : "Aplicar…"}
                      </button>
                    {:else}
                      <button
                        class="btn btn-sm"
                        disabled={busy || revertBlock !== null}
                        title={revertBlock ?? "Ver el script de reversa y deshacerla"}
                        onclick={() => review(item, "down")}
                      >
                        Deshacer…
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}

        {#if records.length > 0}
          <button class="btn btn-sm btn-ghost mt-4" onclick={() => (showLog = !showLog)}>
            <Icon name="chevron" size={11} class="transition-transform {showLog ? 'rotate-180' : ''}" />
            Registro de intentos ({records.length})
          </button>
          {#if showLog}
            <table class="list-table mt-1">
              <thead>
                <tr>
                  <th>Cuándo</th>
                  <th>Versión</th>
                  <th>Sentido</th>
                  <th>Resultado</th>
                  <th>Huella</th>
                </tr>
              </thead>
              <tbody>
                {#each records as record (record.id)}
                  <tr>
                    <td class="text-xs whitespace-nowrap tabular-nums">{when(record.appliedAt)}</td>
                    <td class="font-mono text-xs">{versionLabel(record.version)}</td>
                    <td class="text-xs">{record.direction === "up" ? "aplicar" : "deshacer"}</td>
                    <td class="text-xs">
                      {#if record.succeeded}
                        <span class="tag tag-ok">ok</span>
                      {:else}
                        <span class="tag tag-bad" title={record.error ?? ""}>falló</span>
                      {/if}
                    </td>
                    <td class="font-mono text-xs muted" title={record.checksum}>
                      {shortSum(record.checksum)}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        {/if}
      {/if}
    </div>
  {/if}
</div>

{#if reviewing}
  {@const { item, plan } = reviewing}
  <Modal
    title="{plan.direction === 'up' ? 'Aplicar' : 'Deshacer'} {versionLabel(plan.version)}"
    subtitle="{descriptionText(plan.description)} · {tab.database}"
    size="xl"
    {busy}
    onclose={() => (reviewing = null)}
  >
    <p class="mb-2 text-xs muted">
      Esto es lo que se va a ejecutar, tal como está en el archivo ahora.
      {plan.execution.mode === "atomic"
        ? "Va dentro de una transacción: o queda entera o no queda nada."
        : plan.execution.mode === "selfManaged"
          ? "El script maneja su propia transacción."
          : "Corre sentencia por sentencia, sin transacción."}
    </p>
    {#each plan.warnings as warning (warning)}
      <Alert tone="warn" box class="mb-2">{warning}</Alert>
    {/each}
    {#if item.state === "changed"}
      <Alert tone="warn" box class="mb-2">
        El archivo cambió desde que se aplicó: lo que se deshace es lo que dice el script de
        reversa de hoy.
      </Alert>
    {/if}
    <div
      class="max-h-80 overflow-auto rounded-md border border-zinc-200 bg-zinc-50 p-2 select-text
             dark:border-zinc-700 dark:bg-zinc-700/50"
    >
      <Sql code={plan.execution.steps.length ? plan.execution.steps.join(";\n") + ";" : "-- vacío"} />
    </div>
    <p class="mt-2 text-xs muted" title={plan.checksum}>Huella {shortSum(plan.checksum)}</p>

    {#snippet footer()}
      <button class="btn ml-auto" disabled={busy} onclick={() => (reviewing = null)}>Cancelar</button>
      <button
        class="btn btn-primary"
        disabled={busy || plan.execution.steps.length === 0}
        onclick={confirm}
      >
        {plan.direction === "up" ? "Aplicar" : "Deshacer"}
      </button>
    {/snippet}
  </Modal>
{/if}
