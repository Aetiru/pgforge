<script lang="ts">
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { duration } from "./format";
  import {
    Channel,
    describeError,
    maintenanceCancel,
    maintenancePlan,
    maintenanceRun,
    type MaintenanceEvent,
    type Operation,
    type Target,
  } from "./ipc";

  let {
    profileId,
    target,
    onclose,
  }: {
    profileId: string;
    target: Target;
    onclose: () => void;
  } = $props();

  type Kind = "vacuum" | "analyze" | "reindex";

  let kind = $state<Kind>("vacuum");
  let full = $state(false);
  let freeze = $state(false);
  let analyze = $state(false);
  let concurrently = $state(true);

  let sql = $state("");
  let warning = $state<string | null>(null);
  let planError = $state<string | null>(null);

  let taskId = $state<string | null>(null);
  let log = $state<string[]>([]);
  let outcome = $state<string | null>(null);
  let failed = $state(false);

  const operation = $derived.by<Operation>(() => {
    switch (kind) {
      case "vacuum":
        return { kind: "vacuum", full, freeze, analyze };
      case "analyze":
        return { kind: "analyze" };
      case "reindex":
        return { kind: "reindex", concurrently };
    }
  });

  const running = $derived(taskId !== null);
  const targetLabel = $derived(
    target.kind === "table" ? `${target.schema}.${target.name}` : `base ${target.name}`,
  );

  const KINDS: { value: Kind; label: string; hint: string }[] = [
    { value: "vacuum", label: "VACUUM", hint: "recupera el espacio de las filas muertas" },
    { value: "analyze", label: "ANALYZE", hint: "actualiza las estadísticas del planificador" },
    { value: "reindex", label: "REINDEX", hint: "reconstruye los índices" },
  ];

  // La sentencia exacta se pide al núcleo en vez de armarla acá: así lo que se muestra es
  // literalmente lo que se va a ejecutar, no una reconstrucción parecida.
  $effect(() => {
    const current = operation;
    let cancelled = false;
    maintenancePlan(profileId, current, target)
      .then((plan) => {
        if (cancelled) return;
        sql = plan.sql;
        warning = plan.warning;
        planError = null;
      })
      .catch((error) => {
        if (cancelled) return;
        planError = describeError(error);
        sql = "";
      });
    return () => {
      cancelled = true;
    };
  });

  async function run() {
    log = [];
    outcome = null;
    failed = false;

    const channel = new Channel<MaintenanceEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "started":
          log = [...log, `» ${event.sql}`];
          break;
        case "notice":
          log = [...log, event.message];
          break;
        case "finished":
          outcome = `Terminó en ${duration(event.seconds)}.`;
          taskId = null;
          break;
        case "failed":
          outcome = describeError(event.error);
          failed = true;
          taskId = null;
          break;
      }
    };

    try {
      taskId = await maintenanceRun(profileId, operation, target, channel);
    } catch (error) {
      outcome = describeError(error);
      failed = true;
      taskId = null;
    }
  }

  async function cancel() {
    if (!taskId) return;
    try {
      await maintenanceCancel(taskId);
    } catch (error) {
      outcome = describeError(error);
    }
  }
</script>

<Modal title="Mantenimiento" subtitle={targetLabel} size="lg" busy={running} {onclose}>
  <div class="seg" role="tablist">
    {#each KINDS as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={kind === item.value}
        title={item.hint}
        disabled={running}
        onclick={() => (kind = item.value)}
      >
        {item.label}
      </button>
    {/each}
  </div>
  <p class="mt-1 text-xs muted">{KINDS.find((item) => item.value === kind)?.hint}</p>

  {#if kind === "vacuum"}
    <div class="mt-3 flex flex-wrap gap-4">
      <label class="check">
        <input type="checkbox" bind:checked={full} disabled={running} /> FULL
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={freeze} disabled={running} /> FREEZE
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={analyze} disabled={running} /> ANALYZE
      </label>
    </div>
  {:else if kind === "reindex"}
    <label class="check mt-3">
      <input type="checkbox" bind:checked={concurrently} disabled={running} />
      CONCURRENTLY (no bloquea las escrituras)
    </label>
  {/if}

  {#if warning}
    <Alert tone="warn" box class="mt-3">{warning}</Alert>
  {/if}

  {#if planError}
    <Alert tone="bad" box class="mt-3">{planError}</Alert>
  {:else}
    <pre
      class="mt-3 overflow-x-auto rounded-md border border-zinc-200 bg-zinc-50 px-3 py-2 font-mono
             text-xs select-text dark:border-zinc-800 dark:bg-zinc-800/50">{sql}</pre>
  {/if}

  {#if log.length > 0 || outcome}
    <div
      class="mt-3 max-h-56 overflow-auto rounded-md border border-zinc-200 px-3 py-2 font-mono
             text-xs select-text dark:border-zinc-800"
    >
      {#each log as line, index (index)}
        <div class="whitespace-pre-wrap">{line}</div>
      {/each}
      {#if outcome}
        <div
          class={failed
            ? "text-rose-600 dark:text-rose-400"
            : "text-emerald-600 dark:text-emerald-400"}
        >
          {outcome}
        </div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    {#if running}
      <span class="flex items-center gap-2 text-xs muted">
        <span class="spinner"></span>
        en curso…
      </span>
    {/if}
    <button class="btn ml-auto" onclick={onclose} disabled={running}>Cerrar</button>
    {#if running}
      <button class="btn btn-danger" onclick={cancel}>Cancelar la tarea</button>
    {:else}
      <button class="btn btn-primary" onclick={run} disabled={!sql}>Ejecutar</button>
    {/if}
  {/snippet}
</Modal>
