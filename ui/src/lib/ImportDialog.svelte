<script lang="ts">
  import { t } from "./i18n.svelte";
  import { pickOpen } from "./ipc";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { tasks } from "./tasks.svelte";
  import { dataImportPreview, describeError, type CopyFormat, type ImportSpec } from "./ipc";

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

  const formats = $derived<{ value: CopyFormat; label: string; hint: string }[]>([
    { value: "csv", label: "CSV", hint: t("valores separados por comas") },
    { value: "text", label: t("Texto"), hint: t("el formato de COPY: campos separados por tabulador") },
    { value: "binary", label: t("Binario"), hint: t("el que produce una exportación binaria de pgforge") },
  ]);

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

  const spec = $derived.by<ImportSpec>(() => ({
    schema,
    table,
    columns: [],
    format,
    options: isBinary
      ? { header: false }
      : {
          header: isCsv && header,
          delimiter: delimiter || undefined,
          quote: isCsv && quote ? quote : undefined,
          null: nullText || undefined,
        },
  }));

  // El COPY exacto lo arma el núcleo, no la interfaz: lo que se muestra es lo que se va a ejecutar.
  $effect(() => {
    const current = spec;
    let cancelled = false;
    dataImportPreview(current)
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
    const chosen = await pickOpen({ title: t("Qué archivo importar") });
    if (typeof chosen === "string") path = chosen;
  }

  /**
   * La larga y cierra. Sigue yendo en una sola transacción del lado del servidor, así que cancelar
   * a la mitad no deja la tabla a medio llenar; lo que cambia es que la ventana queda libre.
   */
  async function run() {
    if (!(await confirmMutation(profileId, t("Se van a insertar filas en la tabla.")))) return;

    try {
      await tasks.import({ profileId, database, spec, path });
    } catch (error) {
      previewError = describeError(error);
      return;
    }
    onclose();
  }
</script>

<Modal title={t("Importar")} subtitle="{schema}.{table}" size="lg" {onclose}>
  <div class="seg" role="tablist">
    {#each formats as item (item.value)}
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
  <p class="mt-1 text-xs muted">{formats.find((item) => item.value === format)?.hint}</p>

  <div class="mt-3 flex items-end gap-2">
    <label class="flex min-w-0 flex-1 flex-col gap-1">
      <span class="label">{t("Archivo")}</span>
      <input
        class="field font-mono text-xs"
        bind:value={path}
        readonly
        title={t("La ruta se elige con el botón: solo se aceptan rutas elegidas en el diálogo del sistema")}
      />
    </label>
    <button class="btn" onclick={choose}>{t("Elegir…")}</button>
  </div>

  {#if !isBinary}
    {#if isCsv}
      <label class="check mt-3">
        <input type="checkbox" bind:checked={header} />
        {t("La primera línea trae los nombres de columna (se saltea)")}
      </label>
    {/if}

    <div class="mt-3 flex flex-wrap gap-4">
      <label class="flex flex-col gap-1">
        <span class="label">{t("Delimitador")}</span>
        <input
          class="field w-20 font-mono"
          maxlength="1"
          bind:value={delimiter}
          placeholder={isCsv ? "," : t("tab")}
        />
      </label>
      {#if isCsv}
        <label class="flex flex-col gap-1">
          <span class="label">{t("Comillas")}</span>
          <input
            class="field w-20 font-mono"
            maxlength="1"
            bind:value={quote}
            placeholder={'"'}
          />
        </label>
      {/if}
      <label class="flex flex-col gap-1">
        <span class="label">{t("Texto para NULL")}</span>
        <input
          class="field w-28 font-mono"
          bind:value={nullText}
          placeholder={isCsv ? t("(vacío)") : "\\N"}
        />
      </label>
    </div>
  {/if}

  <Alert tone="warn" box class="mt-3">
    {t("El archivo carga entero o no carga nada: si una fila no entra, se revierte todo. Las columnas del archivo tienen que venir en el orden de la tabla.")}
  </Alert>

  {#if previewError}
    <Alert tone="bad" box class="mt-3">{previewError}</Alert>
  {:else if command}
    <pre
      class="mt-3 max-h-32 overflow-auto rounded-md border border-zinc-200 bg-zinc-50 px-3 py-2
             font-mono text-xs whitespace-pre-wrap select-text dark:border-zinc-700
             dark:bg-zinc-700/50">{command}</pre>
  {/if}

  <p class="mt-2 text-xs muted">
    {t("Corre en segundo plano: se sigue y se cancela desde la vista de procesos.")}
  </p>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>{t("Cerrar")}</button>
    <button class="btn btn-primary" onclick={run} disabled={!path || command === null}>
      {t("Importar")}
    </button>
  {/snippet}
</Modal>
