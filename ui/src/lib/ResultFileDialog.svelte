<script lang="ts">
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { count } from "./format";
  import { describeError, pickSave, sqlWriteFile } from "./ipc";
  import { toJson, toMarkdown } from "./result-export";

  let { columns, rows, types, truncated, onclose }: {
    columns: string[];
    rows: (string | null)[][];
    types: string[] | null;
    truncated: boolean;
    onclose: () => void;
  } = $props();

  const FORMATS = [
    { value: "json", label: "JSON", extension: "json", hint: "un arreglo de objetos, uno por fila" },
    { value: "markdown", label: "Markdown", extension: "md", hint: "una tabla lista para pegar" },
  ] as const;

  let format = $state<(typeof FORMATS)[number]["value"]>("json");
  let error = $state<string | null>(null);
  let busy = $state(false);
  let saved = $state<string | null>(null);

  const current = $derived(FORMATS.find((item) => item.value === format)!);

  async function save() {
    error = null;
    busy = true;
    try {
      // El archivo lo elige un diálogo abierto desde Rust: `sqlWriteFile` solo acepta esa ruta.
      const path = await pickSave({
        title: "Exportar el resultado",
        defaultPath: `resultado.${current.extension}`,
        filters: [{ name: current.label, extensions: [current.extension] }],
      });
      if (!path) return;
      const data = { columns, rows };
      await sqlWriteFile(path, format === "json" ? toJson(data, types) : toMarkdown(data));
      saved = path;
    } catch (e) {
      error = describeError(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Exportar a JSON o Markdown" size="sm" {busy} {onclose}>
  <div class="seg" role="tablist">
    {#each FORMATS as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={format === item.value}
        onclick={() => (format = item.value)}>{item.label}</button
      >
    {/each}
  </div>
  <p class="mt-2 text-xs muted">{current.hint}.</p>
  <p class="mt-2 text-xs muted">
    Se escriben las {count(rows.length)} filas que están cargadas. Para CSV, texto o binario —que
    sí vuelven al servidor y traen todas— usá «Exportar» de la barra.
  </p>

  {#if truncated}
    <Alert tone="warn" box class="mt-3">
      El resultado quedó cortado por el techo de filas: el archivo no tiene todas. Subí el máximo y
      volvé a ejecutar, o exportá en CSV.
    </Alert>
  {/if}
  {#if error}<Alert tone="bad" box class="mt-3">{error}</Alert>{/if}
  {#if saved}<Alert tone="ok" box class="mt-3 break-all">Guardado en {saved}.</Alert>{/if}

  {#snippet footer()}
    <button class="btn ml-auto" disabled={busy} onclick={onclose}>Cerrar</button>
    <button class="btn btn-primary" disabled={busy} onclick={save}>
      <Icon name="download" size={12} />
      Guardar…
    </button>
  {/snippet}
</Modal>
