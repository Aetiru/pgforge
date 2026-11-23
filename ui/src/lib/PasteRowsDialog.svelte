<script lang="ts">
  import Alert from "./Alert.svelte";
  import { t } from "./i18n.svelte";
  import Modal from "./Modal.svelte";
  import { count } from "./format";
  import { parseTsv, planPaste } from "./tsv";

  /**
   * Pegar filas copiadas de Excel (o de cualquier hoja) como filas nuevas de la tabla.
   *
   * Lo pegado no se escribe: pasa a ser altas sin guardar en la grilla, con el mismo «Ver el SQL» y
   * la misma confirmación de siempre. Se pega en un cuadro y no se lee el portapapeles solo: leerlo
   * pide un permiso que la ventana no siempre da, y así se ve exactamente qué se va a agregar.
   */
  let {
    columns,
    onadd,
    onclose,
  }: {
    columns: { name: string; generated: boolean }[];
    onadd: (rows: Map<number, string | null>[]) => void;
    onclose: () => void;
  } = $props();

  let text = $state("");
  /** `null` es «decidir solo»: si la primera fila son nombres de columnas, se toma como encabezado. */
  let header = $state<boolean | null>(null);

  const grid = $derived(parseTsv(text));
  const plan = $derived(text.trim() === "" ? null : planPaste(grid, columns, { header: header ?? undefined }));
  const ready = $derived(plan !== null && typeof plan !== "string");

  function add() {
    if (plan === null || typeof plan === "string") return;
    onadd(plan.rows);
    onclose();
  }
</script>

<Modal title={t("Pegar filas")} subtitle={t("Desde Excel o cualquier hoja")} size="lg" {onclose}>
  <!-- svelte-ignore a11y_autofocus -->
  <textarea
    class="field h-40 w-full font-mono text-xs"
    placeholder={t("Copiá las celdas en la hoja y pegalas acá (Ctrl+V)")}
    autofocus
    spellcheck="false"
    bind:value={text}
  ></textarea>

  <div class="mt-2 flex flex-wrap items-center gap-x-5 gap-y-1 text-xs">
    <label class="check">
      <input
        type="checkbox"
        checked={header ?? (plan !== null && typeof plan !== "string" && plan.usedHeader)}
        onchange={(event) => (header = event.currentTarget.checked)}
      />
      {t("La primera fila son los nombres de las columnas")}
    </label>
    <span class="muted">
      {t("Sin encabezado se pega en el orden de las columnas, saltando las que calcula el servidor.")}
    </span>
  </div>

  {#if typeof plan === "string"}
    <Alert tone="warn" box class="mt-3">{plan}</Alert>
  {:else if plan}
    <p class="mt-3 text-xs">
      <b>{count(plan.rows.length)}</b>
      {plan.rows.length === 1 ? t("fila en") : t("filas en")}
      {#each plan.columns as name, index (name)}<code class="font-mono">{name}</code
        >{index < plan.columns.length - 1 ? ", " : ""}{/each}.
      {t("Las celdas vacías quedan en NULL.")}
    </p>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>{t("Cancelar")}</button>
    <button class="btn btn-primary" disabled={!ready} onclick={add}>
      {plan && typeof plan !== "string" ? t("Agregar {n} filas", { n: plan.rows.length }) : t("Agregar filas")}
    </button>
  {/snippet}
</Modal>
