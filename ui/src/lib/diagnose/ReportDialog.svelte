<script lang="ts">
  import { t } from "../i18n.svelte";
  import Alert from "../Alert.svelte";
  import Icon from "../Icon.svelte";
  import Modal from "../Modal.svelte";
  import { describeError, pickSave, sqlWriteFile } from "../ipc";
  import { toHtml, toMarkdown } from "../health-report";
  import { collectReport } from "./report-export";
  import type { Metrics } from "../ipc";

  let {
    profileId,
    server,
    database,
    metrics,
    onclose,
  }: {
    profileId: string;
    server: string;
    database: string;
    metrics: Metrics | null;
    onclose: () => void;
  } = $props();

  const formats = $derived([
    { value: "html", label: "HTML", extension: "html", hint: t("un solo archivo que se abre en cualquier navegador, en claro u oscuro") },
    { value: "markdown", label: "Markdown", extension: "md", hint: t("para pegar en un ticket, un wiki o un README") },
  ] as const);

  let format = $state<"html" | "markdown">("html");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let saved = $state<string | null>(null);

  const current = $derived(formats.find((item) => item.value === format)!);

  async function save() {
    error = null;
    saved = null;
    busy = true;
    try {
      // La ruta la elige un diálogo abierto desde Rust: `sqlWriteFile` solo acepta esa.
      const stamp = new Date().toISOString().slice(0, 10);
      const path = await pickSave({
        title: t("Guardar el informe de salud"),
        defaultPath: `salud-${database}-${stamp}.${current.extension}`,
        filters: [{ name: current.label, extensions: [current.extension] }],
      });
      if (!path) return;
      const input = await collectReport(profileId, server, database, metrics);
      await sqlWriteFile(path, format === "html" ? toHtml(input) : toMarkdown(input));
      saved = path;
    } catch (e) {
      error = describeError(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t("Informe de salud")} size="sm" {busy} {onclose}>
  <div class="seg" role="tablist">
    {#each formats as item (item.value)}
      <button class="seg-item" role="tab" aria-selected={format === item.value} onclick={() => (format = item.value)}>
        {item.label}
      </button>
    {/each}
  </div>
  <p class="mt-2 text-xs muted">{current.hint}.</p>
  <p class="mt-2 text-xs muted">
    {t("Vuelve a leer el servidor y escribe solo lo que pide algo: avisos, secuencias y tablas para revisar, espacio y configuración. Lo que el rol no puede ver queda anotado en el informe.")}
  </p>
  {#if error}<Alert tone="bad" box class="mt-3">{error}</Alert>{/if}
  {#if saved}<Alert tone="ok" box class="mt-3 break-all">{t("Guardado en {path}.", { path: saved })}</Alert>{/if}

  {#snippet footer()}
    <button class="btn ml-auto" disabled={busy} onclick={onclose}>{t("Cerrar")}</button>
    <button class="btn btn-primary" disabled={busy} onclick={save}>
      {#if busy}<span class="spinner"></span>{:else}<Icon name="download" size={12} />{/if}
      {t("Generar y guardar…")}
    </button>
  {/snippet}
</Modal>
