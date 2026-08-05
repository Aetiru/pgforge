<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { bytes, duration } from "./format";
  import {
    Channel,
    dataCopyCancel,
    dataExportPreview,
    dataExportRun,
    describeError,
    type CopyFormat,
    type ExportEvent,
    type ExportSpec,
  } from "./ipc";

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

  const FORMATS: { value: CopyFormat; label: string; hint: string; extension: string }[] = [
    {
      value: "csv",
      label: "CSV",
      hint: "valores separados por comas, con comillas donde hace falta",
      extension: "csv",
    },
    {
      value: "text",
      label: "Texto",
      hint: "el formato de COPY: campos separados por tabulador, sin comillas",
      extension: "tsv",
    },
    {
      value: "binary",
      label: "Binario",
      hint: "compacto, pero no portable entre versiones de PostgreSQL",
      extension: "bin",
    },
  ];

  let format = $state<CopyFormat>("csv");
  let path = $state("");
  let header = $state(true);
  let delimiter = $state("");
  let quote = $state("");
  let nullText = $state("");

  let command = $state<string | null>(null);
  let previewError = $state<string | null>(null);

  let taskId = $state<string | null>(null);
  let progress = $state<number | null>(null);
  let outcome = $state<string | null>(null);
  let failed = $state(false);

  const running = $derived(taskId !== null);
  const isCsv = $derived(format === "csv");
  const isBinary = $derived(format === "binary");

  const spec = $derived.by<ExportSpec>(() => ({
    source: { kind: "table", schema, table, columns: [] },
    format,
    // El binario no admite ninguna de estas opciones; se mandan vacías y el núcleo las valida.
    options: isBinary
      ? { header: false }
      : {
          header: isCsv && header,
          delimiter: delimiter || undefined,
          quote: isCsv && quote ? quote : undefined,
          null: nullText || undefined,
        },
  }));

  // El COPY exacto se le pide al núcleo, no se arma acá: así lo que se muestra es literalmente lo
  // que se va a ejecutar. Mismo criterio que el diálogo de backup.
  $effect(() => {
    const current = spec;
    let cancelled = false;
    dataExportPreview(current)
      .then((preview) => {
        if (cancelled) return;
        command = preview.sql;
        previewError = null;
      })
      .catch((error) => {
        if (cancelled) return;
        previewError = describeError(error);
        command = null;
      });
    return () => {
      cancelled = true;
    };
  });

  async function choose() {
    const chosen = await save({
      title: "Dónde guardar la exportación",
      defaultPath: `${table}.${FORMATS.find((f) => f.value === format)?.extension}`,
    });
    if (typeof chosen === "string") path = chosen;
  }

  async function run() {
    progress = null;
    outcome = null;
    failed = false;

    const channel = new Channel<ExportEvent>();
    channel.onmessage = (event) => {
      switch (event.type) {
        case "started":
          progress = 0;
          break;
        case "progress":
          progress = event.bytes;
          break;
        case "finished":
          outcome = `Listo: ${event.path} (${bytes(event.bytes)}) en ${duration(event.seconds)}.`;
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
      taskId = await dataExportRun(profileId, spec, path, channel, database);
    } catch (error) {
      outcome = describeError(error);
      failed = true;
      taskId = null;
    }
  }

  async function cancel() {
    if (!taskId) return;
    try {
      await dataCopyCancel(taskId);
    } catch (error) {
      outcome = describeError(error);
    }
  }
</script>

<Modal title="Exportar" subtitle="{schema}.{table}" size="lg" busy={running} {onclose}>
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
      <span class="label">Archivo</span>
      <input class="field font-mono text-xs" bind:value={path} disabled={running} />
    </label>
    <button class="btn" onclick={choose} disabled={running}>Elegir…</button>
  </div>

  {#if !isBinary}
    {#if isCsv}
      <label class="check mt-3">
        <input type="checkbox" bind:checked={header} disabled={running} />
        Primera línea con los nombres de columna
      </label>
    {/if}

    <div class="mt-3 flex flex-wrap gap-4">
      <label class="flex flex-col gap-1">
        <span class="label">Delimitador</span>
        <input
          class="field w-20 font-mono"
          maxlength="1"
          bind:value={delimiter}
          disabled={running}
          placeholder={isCsv ? "," : "tab"}
        />
      </label>
      {#if isCsv}
        <label class="flex flex-col gap-1">
          <span class="label">Comillas</span>
          <input
            class="field w-20 font-mono"
            maxlength="1"
            bind:value={quote}
            disabled={running}
            placeholder={'"'}
          />
        </label>
      {/if}
      <label class="flex flex-col gap-1">
        <span class="label">Texto para NULL</span>
        <input
          class="field w-28 font-mono"
          bind:value={nullText}
          disabled={running}
          placeholder={isCsv ? "(vacío)" : "\\N"}
        />
      </label>
    </div>
  {/if}

  {#if previewError}
    <Alert tone="bad" box class="mt-3">{previewError}</Alert>
  {:else if command}
    <pre
      class="mt-3 max-h-32 overflow-auto rounded-md border border-zinc-200 bg-zinc-50 px-3 py-2
             font-mono text-xs whitespace-pre-wrap select-text dark:border-zinc-800
             dark:bg-zinc-800/50">{command}</pre>
  {/if}

  {#if outcome}
    <div
      class="mt-3 rounded-md border border-zinc-200 px-3 py-2 font-mono text-xs select-text
             dark:border-zinc-800 {failed
        ? 'text-rose-600 dark:text-rose-400'
        : 'text-emerald-600 dark:text-emerald-400'}"
    >
      {outcome}
    </div>
  {/if}

  {#snippet footer()}
    {#if running}
      <span class="flex items-center gap-2 text-xs muted">
        <span class="spinner"></span>
        {progress === null ? "en curso…" : `${bytes(progress)} exportados`}
      </span>
    {/if}
    <button class="btn ml-auto" onclick={onclose} disabled={running}>Cerrar</button>
    {#if running}
      <button class="btn btn-danger" onclick={cancel}>Cancelar</button>
    {:else}
      <button class="btn btn-primary" onclick={run} disabled={!path || command === null}>
        Exportar
      </button>
    {/if}
  {/snippet}
</Modal>
