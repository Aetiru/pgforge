<script lang="ts">
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

<div class="fixed inset-0 z-10 flex items-center justify-center bg-black/40 p-4">
  <div
    class="flex max-h-[85vh] w-full max-w-2xl flex-col rounded-lg bg-white shadow-xl dark:bg-zinc-900"
    role="dialog"
    aria-modal="true"
    aria-label="Mantenimiento"
  >
    <h2 class="border-b border-zinc-200 px-5 py-3 text-base font-medium dark:border-zinc-800">
      Mantenimiento de {targetLabel}
    </h2>

    <div class="space-y-3 px-5 py-4 text-sm">
      <div class="flex gap-2">
        {#each [["vacuum", "VACUUM"], ["analyze", "ANALYZE"], ["reindex", "REINDEX"]] as [value, text] (value)}
          <button
            class="btn {kind === value ? 'btn-primary' : ''}"
            disabled={running}
            onclick={() => (kind = value as Kind)}
          >
            {text}
          </button>
        {/each}
      </div>

      {#if kind === "vacuum"}
        <div class="flex flex-wrap gap-4 text-xs text-zinc-600 dark:text-zinc-300">
          <label class="flex items-center gap-1.5">
            <input type="checkbox" bind:checked={full} disabled={running} /> FULL
          </label>
          <label class="flex items-center gap-1.5">
            <input type="checkbox" bind:checked={freeze} disabled={running} /> FREEZE
          </label>
          <label class="flex items-center gap-1.5">
            <input type="checkbox" bind:checked={analyze} disabled={running} /> ANALYZE
          </label>
        </div>
      {:else if kind === "reindex"}
        <label class="flex items-center gap-1.5 text-xs text-zinc-600 dark:text-zinc-300">
          <input type="checkbox" bind:checked={concurrently} disabled={running} />
          CONCURRENTLY (no bloquea las escrituras)
        </label>
      {/if}

      {#if warning}
        <p
          class="rounded border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-900
                 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-200"
        >
          {warning}
        </p>
      {/if}

      {#if planError}
        <p class="text-xs text-rose-600 dark:text-rose-400">{planError}</p>
      {:else}
        <pre
          class="select-text overflow-x-auto rounded bg-zinc-100 px-3 py-2 font-mono text-xs
                 dark:bg-zinc-800">{sql}</pre>
      {/if}

      {#if log.length > 0 || outcome}
        <div
          class="max-h-56 select-text overflow-auto rounded border border-zinc-200 px-3 py-2
                 font-mono text-xs dark:border-zinc-800"
        >
          {#each log as line, index (index)}
            <div class="whitespace-pre-wrap">{line}</div>
          {/each}
          {#if outcome}
            <div class={failed ? "text-rose-600 dark:text-rose-400" : "text-emerald-600"}>
              {outcome}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div
      class="flex justify-end gap-2 border-t border-zinc-200 px-5 py-3 dark:border-zinc-800"
    >
      <button class="btn" onclick={onclose} disabled={running}>Cerrar</button>
      {#if running}
        <button class="btn" onclick={cancel}>Cancelar la tarea</button>
      {:else}
        <button class="btn btn-primary" onclick={run} disabled={!sql}>Ejecutar</button>
      {/if}
    </div>
  </div>
</div>
