<script lang="ts">
  import { t } from "./i18n.svelte";
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { previewLines } from "./editor/rename-alias";
  import { validateAliasName, type AliasAt } from "./sql-rename";

  /**
   * Renombrar un alias (o una CTE) en toda su sentencia. Muestra cuántas apariciones se cambian y las
   * primeras líneas afectadas antes de aplicar: el alcance es la sentencia entera y no cada
   * subconsulta (ver `sql-rename.ts`), así que si un mismo alias se repite en dos subconsultas se
   * cambian las dos, y el diálogo es donde eso se ve.
   */
  let {
    text,
    pos,
    info,
    onapply,
    onclose,
  }: {
    text: string;
    pos: number;
    info: AliasAt;
    onapply: (newName: string) => void;
    onclose: () => void;
  } = $props();

  let name = $state(untrack(() => info.name));

  const problem = $derived(name === info.name ? t("Es el mismo nombre.") : validateAliasName(text, pos, name));
  const lines = $derived(problem === null ? previewLines(text, info.occurrences, name.trim()) : []);
  const total = $derived(info.occurrences.length);

  function submit() {
    if (problem === null) onapply(name.trim());
  }
</script>

<Modal title={info.kind === "cte" ? t("Renombrar la CTE") : t("Renombrar el alias")} size="sm" {onclose}>
  <label class="label" for="rename-alias-name">{t("Nombre nuevo")}</label>
  <input
    id="rename-alias-name"
    class="field w-full font-mono"
    data-autofocus
    bind:value={name}
    onkeydown={(event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        submit();
      }
    }}
  />

  {#if problem}
    <p class="mt-1.5 text-xs text-rose-600 dark:text-rose-400">{t(problem)}</p>
  {:else}
    <p class="mt-1.5 text-xs muted">
      {total === 1
        ? t("Se cambia 1 aparición en esta sentencia.")
        : t("Se cambian {n} apariciones en esta sentencia.", { n: total })}
    </p>
  {/if}

  {#if lines.length > 0}
    <div class="mt-2 max-h-40 overflow-auto rounded border border-zinc-200 p-1.5 font-mono text-[11px] dark:border-zinc-700">
      {#each lines as line (line.line)}
        <div class="flex gap-2">
          <span class="w-5 shrink-0 text-right tabular-nums muted">{line.line}</span>
          <span class="min-w-0 truncate" title={line.before}>{line.after}</span>
        </div>
      {/each}
    </div>
  {/if}

  <p class="mt-3 text-xs muted">
    {t("Vale para toda la sentencia, no solo para una subconsulta. Se deshace con Ctrl+Z.")}
  </p>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>{t("Cancelar")}</button>
    <button class="btn btn-primary" disabled={problem !== null} onclick={submit}>{t("Renombrar")}</button>
  {/snippet}
</Modal>
