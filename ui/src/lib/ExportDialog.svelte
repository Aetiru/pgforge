<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { tasks } from "./tasks.svelte";
  import {
    dataExportPreview,
    describeError,
    type CopyFormat,
    type ExportSource,
    type ExportSpec,
  } from "./ipc";

  /**
   * De dónde salen las filas: una tabla del árbol o el resultado de la consulta que se acaba de
   * ejecutar. El diálogo es el mismo porque la pregunta es la misma —formato, archivo, opciones—;
   * lo único que cambia es qué `COPY` arma el núcleo, y eso ya lo decide `ExportSource`.
   */
  let {
    profileId,
    database,
    source,
    onclose,
  }: {
    profileId: string;
    database: string;
    source: ExportSource;
    onclose: () => void;
  } = $props();

  /** Nombre para el título y para proponer el archivo. */
  const label = $derived(
    source.kind === "table" ? `${source.schema}.${source.table}` : "resultado de la consulta",
  );
  const fileName = $derived(source.kind === "table" ? source.table : "consulta");

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

  const isCsv = $derived(format === "csv");
  const isBinary = $derived(format === "binary");

  const spec = $derived.by<ExportSpec>(() => ({
    source,
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
      defaultPath: `${fileName}.${FORMATS.find((f) => f.value === format)?.extension}`,
    });
    if (typeof chosen === "string") path = chosen;
  }

  /** La larga y cierra: exportar una tabla grande no puede tener la ventana esperando. */
  function run() {
    tasks.export({ profileId, database, target: label, spec, path });
    onclose();
  }
</script>

<Modal title="Exportar" subtitle={label} size="lg" {onclose}>
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
      <span class="label">Archivo</span>
      <input class="field font-mono text-xs" bind:value={path} />
    </label>
    <button class="btn" onclick={choose}>Elegir…</button>
  </div>

  {#if !isBinary}
    {#if isCsv}
      <label class="check mt-3">
        <input type="checkbox" bind:checked={header} />
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
            placeholder={'"'}
          />
        </label>
      {/if}
      <label class="flex flex-col gap-1">
        <span class="label">Texto para NULL</span>
        <input
          class="field w-28 font-mono"
          bind:value={nullText}
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

  <p class="mt-2 text-xs muted">
    Corre en segundo plano: se sigue y se cancela desde la vista de procesos.
  </p>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
    <button class="btn btn-primary" onclick={run} disabled={!path || command === null}>
      Exportar
    </button>
  {/snippet}
</Modal>
