<script lang="ts">
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { duration } from "./format";
  import {
    Channel,
    describeError,
    restoreCancel,
    restorePlan,
    restoreRun,
    type BackupFormat,
    type RestoreEvent,
    type RestoreOptions,
  } from "./ipc";

  let {
    profileId,
    database,
    onclose,
  }: {
    profileId: string;
    database: string;
    onclose: () => void;
  } = $props();

  // El formato plano queda afuera a propósito: es un script SQL que se restaura con psql, no con
  // pg_restore. Los otros tres son los que esta herramienta sabe leer.
  const FORMATS: { value: BackupFormat; label: string; hint: string }[] = [
    {
      value: "custom",
      label: "Custom",
      hint: "el formato propio de pg_dump, comprimido",
    },
    {
      value: "directory",
      label: "Directorio",
      hint: "un directorio con un archivo por tabla; el único que se puede paralelizar",
    },
    { value: "tar", label: "Tar", hint: "un tar sin comprimir" },
  ];

  /** Qué se restaura. Las tres opciones son excluyentes, así que van como una sola. */
  type Contents = "all" | "schema" | "data";

  let format = $state<BackupFormat>("custom");
  let source = $state("");
  // Base destino. Arranca en la base seleccionada, pero se puede cambiar para restaurar en otra.
  // Copia tomada una sola vez, como el resto de los diálogos de mutación.
  let target = $state(untrack(() => database));
  let contents = $state<Contents>("all");
  let clean = $state(false);
  let ifExists = $state(false);
  let create = $state(false);
  let noOwner = $state(false);
  let noPrivileges = $state(false);
  let singleTransaction = $state(false);
  let jobs = $state<number | null>(null);

  let command = $state<string[]>([]);
  let warning = $state<string | null>(null);
  let planError = $state<string | null>(null);

  let taskId = $state<string | null>(null);
  let log = $state<string[]>([]);
  let outcome = $state<string | null>(null);
  let failed = $state(false);
  // Terminó, pero pg_restore ignoró algún error: ni éxito limpio ni fallo.
  let finishedWithWarnings = $state(false);

  const running = $derived(taskId !== null);
  const isDirectory = $derived(format === "directory");
  const admiteJobs = $derived(format === "custom" || format === "directory");

  const options = $derived.by<RestoreOptions>(() => ({
    source,
    format,
    database: target,
    schemas: [],
    tables: [],
    schemaOnly: contents === "schema",
    dataOnly: contents === "data",
    clean,
    // «si existe» no tiene sentido sin «limpiar»: el núcleo lo rechazaría, así que ni se manda.
    ifExists: clean ? ifExists : false,
    create,
    noOwner,
    noPrivileges,
    singleTransaction,
    jobs: admiteJobs ? jobs : null,
  }));

  // La línea exacta se le pide al núcleo en vez de armarla acá: así lo que se muestra es
  // literalmente lo que se va a ejecutar. Mismo criterio que el diálogo de backup.
  $effect(() => {
    const current = options;
    if (!current.source || !current.database) {
      command = [];
      warning = null;
      planError = null;
      return;
    }

    let cancelled = false;
    restorePlan(profileId, current)
      .then((plan) => {
        if (cancelled) return;
        command = plan.command;
        warning = plan.warning;
        planError = null;
      })
      .catch((error) => {
        if (cancelled) return;
        planError = describeError(error);
        command = [];
      });
    return () => {
      cancelled = true;
    };
  });

  async function choose() {
    const chosen = isDirectory
      ? await open({ directory: true, title: "El directorio del backup a restaurar" })
      : await open({ title: "El archivo del backup a restaurar" });
    if (typeof chosen === "string") source = chosen;
  }

  async function run() {
    log = [];
    outcome = null;
    failed = false;
    finishedWithWarnings = false;

    const channel = new Channel<RestoreEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "started":
          log = [...log, `» ${event.command.join(" ")}`];
          break;
        case "progress":
          log = [...log, event.message];
          break;
        case "finished":
          outcome =
            `Restaurado sobre ${event.database} en ${duration(event.seconds)}.` +
            // pg_restore ignora errores por el camino (sin «una sola transacción»): el más común es
            // un dump de una versión más nueva que el servidor. Se avisa y se deja mirar el registro.
            (event.ignoredErrors > 0
              ? ` Se ignoraron ${event.ignoredErrors} ${event.ignoredErrors === 1 ? "error" : "errores"} — revisá el registro de arriba.`
              : "");
          finishedWithWarnings = event.ignoredErrors > 0;
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
      taskId = await restoreRun(profileId, options, channel);
    } catch (error) {
      outcome = describeError(error);
      failed = true;
      taskId = null;
    }
  }

  async function cancel() {
    if (!taskId) return;
    try {
      await restoreCancel(taskId);
    } catch (error) {
      outcome = describeError(error);
    }
  }
</script>

<Modal title="Restaurar" subtitle="base {target}" size="lg" busy={running} {onclose}>
  <div class="seg" role="tablist">
    {#each FORMATS as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={format === item.value}
        title={item.hint}
        disabled={running}
        onclick={() => (format = item.value)}
      >
        {item.label}
      </button>
    {/each}
  </div>
  <p class="mt-1 text-xs muted">{FORMATS.find((item) => item.value === format)?.hint}</p>

  <div class="mt-3 flex items-end gap-2">
    <label class="flex min-w-0 flex-1 flex-col gap-1">
      <span class="label">{isDirectory ? "Directorio del backup" : "Archivo del backup"}</span>
      <input class="field font-mono text-xs" bind:value={source} disabled={running} />
    </label>
    <button class="btn" onclick={choose} disabled={running}>Elegir…</button>
  </div>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">Base destino</span>
    <input class="field" bind:value={target} disabled={running || create} />
    {#if create}
      <span class="text-xs muted">Con «crear la base» el destino lo define el propio backup.</span>
    {/if}
  </label>

  <div class="mt-3">
    <span class="label">Qué restaurar</span>
    <div class="mt-1 flex flex-col gap-1.5">
      <label class="check">
        <input type="radio" value="all" bind:group={contents} disabled={running} />
        El esquema y los datos
      </label>
      <label class="check">
        <input type="radio" value="schema" bind:group={contents} disabled={running} />
        Solo el esquema
      </label>
      <label class="check">
        <input type="radio" value="data" bind:group={contents} disabled={running} />
        Solo los datos
      </label>
    </div>
  </div>

  <div class="mt-3 flex flex-wrap gap-x-4 gap-y-1.5">
    <label class="check">
      <input type="checkbox" bind:checked={clean} disabled={running} />
      Limpiar (eliminar cada objeto antes de recrearlo)
    </label>
    {#if clean}
      <label class="check">
        <input type="checkbox" bind:checked={ifExists} disabled={running} />
        …y no fallar si todavía no existe
      </label>
    {/if}
    <label class="check">
      <input type="checkbox" bind:checked={create} disabled={running} />
      Crear la base destino
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={singleTransaction} disabled={running} />
      Una sola transacción (revierte si algo falla)
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={noOwner} disabled={running} />
      Sin los dueños
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={noPrivileges} disabled={running} />
      Sin los privilegios
    </label>
  </div>

  {#if admiteJobs}
    <label class="mt-3 flex flex-col gap-1">
      <span class="label">Trabajos en paralelo</span>
      <input
        class="field w-24"
        type="number"
        min="1"
        bind:value={jobs}
        disabled={running || singleTransaction}
        placeholder="1"
      />
    </label>
  {/if}

  {#if warning}
    <Alert tone="warn" box class="mt-3">{warning}</Alert>
  {/if}

  {#if planError}
    <Alert tone="bad" box class="mt-3">{planError}</Alert>
  {:else if command.length > 0}
    <pre
      class="mt-3 max-h-32 overflow-auto rounded-md border border-zinc-200 bg-zinc-50 px-3 py-2
             font-mono text-xs whitespace-pre-wrap select-text dark:border-zinc-800
             dark:bg-zinc-800/50">{command.join(" ")}</pre>
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
            : finishedWithWarnings
              ? "text-amber-600 dark:text-amber-400"
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
      <button class="btn btn-danger" onclick={cancel}>Cancelar el restore</button>
    {:else}
      <button class="btn btn-primary" onclick={run} disabled={command.length === 0}>
        Restaurar
      </button>
    {/if}
  {/snippet}
</Modal>
