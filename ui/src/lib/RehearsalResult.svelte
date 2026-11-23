<script lang="ts">
  import { t } from "./i18n.svelte";
  import Alert from "./Alert.svelte";
  import type { RehearsalReport } from "./ipc";
  import {
    groupLocks,
    isHeavyLock,
    lockNote,
    notRevertedNote,
    stepText,
    verdict,
  } from "./rehearsal-view";

  /**
   * Qué habría pasado, según el ensayo: el veredicto, lo que hizo cada sentencia, los locks que
   * tomaría y lo que el `ROLLBACK` no deshace. Se usa tanto en la pestaña de consulta como bajo la
   * vista previa de un diálogo, así que no sabe de dónde salió el script.
   */
  let { report }: { report: RehearsalReport } = $props();

  const result = $derived(verdict(report));
  const groups = $derived(groupLocks(report.locks));
</script>

<div class="space-y-3 text-sm">
  <Alert box tone={result.tone}>
    <strong>{result.title}.</strong>
    {result.detail}
  </Alert>

  {#if report.blocked.length > 0}
    <ul class="space-y-1">
      {#each report.blocked as blocker (blocker.index)}
        <li class="text-xs">
          <span class="muted">{t("Línea {line}:", { line: blocker.line })}</span>
          <code class="select-text font-mono">{blocker.statement}</code>
          <span class="block text-red-600 dark:text-red-400">{blocker.reason}.</span>
        </li>
      {/each}
    </ul>
  {:else}
    <table class="list-table w-full text-xs">
      <thead>
        <tr><th class="w-8 text-left">#</th><th class="text-left">{t("Sentencia")}</th><th class="text-left">{t("Resultado")}</th></tr>
      </thead>
      <tbody>
        {#each report.steps as step (step.index)}
          <tr>
            <td class="muted">{step.index + 1}</td>
            <td class="max-w-[16rem] truncate font-mono" title={step.statement}>{step.statement}</td>
            <td class={step.error ? "text-red-600 dark:text-red-400" : ""}>{stepText(step)}</td>
          </tr>
        {/each}
        {#if report.total > report.steps.length}
          <tr>
            <td></td>
            <td colspan="2" class="muted">
              {t("{count} sin ejecutar.", { count: report.total - report.steps.length })}
            </td>
          </tr>
        {/if}
      </tbody>
    </table>

    <section>
      <h3 class="label mb-1">{t("Locks que tomaría")}</h3>
      {#if !report.locksAvailable}
        <p class="muted text-xs">
          {t("No se pudieron leer: una sentencia falló y la transacción quedó abortada.")}
        </p>
      {:else if groups.length === 0}
        <p class="muted text-xs">{t("Ninguno que valga la pena mencionar.")}</p>
      {:else}
        <ul class="space-y-1 text-xs">
          {#each groups as group (group.relation ?? "—")}
            <li>
              <span class="font-mono">{group.relation ?? t("(sin tabla)")}</span>
              {#each group.modes as mode (mode)}
                <span
                  class="tag {isHeavyLock(mode) ? 'tag-warn' : 'tag-neutral'} ml-1"
                  title={lockNote(mode)}>{mode.replace(/Lock$/, "")}</span
                >
              {/each}
              {#if group.waiting}
                <span class="tag tag-bad ml-1">{t("en espera")}</span>
              {/if}
              {#if isHeavyLock(group.heaviest)}
                <span class="muted block">{lockNote(group.heaviest)}: {t("en producción, ejecutarlo mantiene esto mientras dure la transacción.")}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}

  {#if report.caveats.length > 0}
    <ul class="space-y-1 text-xs">
      {#each report.caveats as caveat (caveat.index + caveat.kind)}
        <li><span class="muted">{t("Sentencia {n}:", { n: caveat.index + 1 })}</span> {caveat.message}.</li>
      {/each}
    </ul>
  {/if}

  <p class="muted text-xs">{notRevertedNote()}</p>
</div>
