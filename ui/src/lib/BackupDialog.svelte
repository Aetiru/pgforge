<script lang="ts">
  import { open, save } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { tasks } from "./tasks.svelte";
  import { backupPlan, describeError, type BackupFormat, type BackupOptions } from "./ipc";

  let {
    profileId,
    database,
    onclose,
  }: {
    profileId: string;
    database: string;
    onclose: () => void;
  } = $props();

  const FORMATS: { value: BackupFormat; label: string; hint: string; extension: string }[] = [
    {
      value: "custom",
      label: "Custom",
      hint: "el formato de pg_restore: comprimido y con restore selectivo",
      extension: "dump",
    },
    {
      value: "plain",
      label: "Plano",
      hint: "un script SQL, se restaura con psql",
      extension: "sql",
    },
    {
      value: "directory",
      label: "Directorio",
      hint: "un archivo por tabla; el único que se puede paralelizar",
      extension: "",
    },
    { value: "tar", label: "Tar", hint: "un tar sin comprimir", extension: "tar" },
  ];

  /** Qué se lleva el backup. Las tres opciones son excluyentes, así que van como una sola. */
  type Contents = "all" | "schema" | "data";

  let format = $state<BackupFormat>("custom");
  let path = $state("");
  let contents = $state<Contents>("all");
  let noOwner = $state(false);
  let noPrivileges = $state(false);
  let compression = $state<number | null>(null);
  let jobs = $state<number | null>(null);

  let command = $state<string[]>([]);
  let warning = $state<string | null>(null);
  let planError = $state<string | null>(null);

  const isDirectory = $derived(format === "directory");
  const admiteCompresion = $derived(format === "custom" || format === "directory");

  const options = $derived.by<BackupOptions>(() => ({
    database,
    format,
    path,
    schemas: [],
    excludeSchemas: [],
    tables: [],
    schemaOnly: contents === "schema",
    dataOnly: contents === "data",
    noOwner,
    noPrivileges,
    compression: admiteCompresion ? compression : null,
    jobs: isDirectory ? jobs : null,
  }));

  // La línea exacta se le pide al núcleo en vez de armarla acá: así lo que se muestra es
  // literalmente lo que se va a ejecutar. Mismo criterio que el diálogo de mantenimiento.
  $effect(() => {
    const current = options;
    if (!current.path) {
      command = [];
      warning = null;
      planError = null;
      return;
    }

    let cancelled = false;
    backupPlan(profileId, current)
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
      ? await open({ directory: true, title: "Dónde guardar el backup" })
      : await save({
          title: "Guardar el backup",
          defaultPath: `${database}.${FORMATS.find((f) => f.value === format)?.extension}`,
        });
    if (typeof chosen === "string") path = chosen;
  }

  /** Lo larga y cierra: el backup sigue corriendo y se mira desde la vista de procesos. */
  function run() {
    tasks.backup({ profileId, options });
    onclose();
  }
</script>

<Modal title="Backup" subtitle="base {database}" size="lg" {onclose}>
  <div class="seg" role="tablist">
    {#each FORMATS as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={format === item.value}
        title={item.hint}
        onclick={() => (format = item.value)}
      >
        {item.label}
      </button>
    {/each}
  </div>
  <p class="mt-1 text-xs muted">{FORMATS.find((item) => item.value === format)?.hint}</p>

  <div class="mt-3 flex items-end gap-2">
    <label class="flex min-w-0 flex-1 flex-col gap-1">
      <span class="label">{isDirectory ? "Directorio" : "Archivo"}</span>
      <input class="field font-mono text-xs" bind:value={path} />
    </label>
    <button class="btn" onclick={choose}>Elegir…</button>
  </div>

  <div class="mt-3">
    <span class="label">Qué incluir</span>
    <div class="mt-1 flex flex-col gap-1.5">
      <label class="check">
        <input type="radio" value="all" bind:group={contents} />
        El esquema y los datos
      </label>
      <label class="check">
        <input type="radio" value="schema" bind:group={contents} />
        Solo el esquema
      </label>
      <label class="check">
        <input type="radio" value="data" bind:group={contents} />
        Solo los datos
      </label>
    </div>
  </div>

  <div class="mt-3 flex flex-wrap gap-4">
    <label class="check">
      <input type="checkbox" bind:checked={noOwner} />
      Sin los dueños (para restaurar con otro rol)
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={noPrivileges} />
      Sin los privilegios
    </label>
  </div>

  {#if admiteCompresion || isDirectory}
    <div class="mt-3 flex flex-wrap gap-4">
      {#if admiteCompresion}
        <label class="flex flex-col gap-1">
          <span class="label">Compresión (0 a 9)</span>
          <input
            class="field w-24"
            type="number"
            min="0"
            max="9"
            bind:value={compression}
            placeholder="por omisión"
          />
        </label>
      {/if}
      {#if isDirectory}
        <label class="flex flex-col gap-1">
          <span class="label">Trabajos en paralelo</span>
          <input
            class="field w-24"
            type="number"
            min="1"
            bind:value={jobs}
            placeholder="1"
          />
        </label>
      {/if}
    </div>
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

  <p class="mt-2 text-xs muted">
    Corre en segundo plano: se sigue y se cancela desde la vista de procesos.
  </p>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
    <button class="btn btn-primary" onclick={run} disabled={command.length === 0}>
      Hacer el backup
    </button>
  {/snippet}
</Modal>
