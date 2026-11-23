<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Sql from "./Sql.svelte";
  import { t } from "./i18n.svelte";
  import {
    cellText,
    countDataRisks,
    dataScriptOf,
    filterDataStatements,
    isChanged,
    keyLabel,
    summary,
  } from "./data-compare-script";
  import { openQuery } from "./query.svelte";
  import type { DataCompareTab } from "./data-compare.svelte";
  import type { DataRowDiff, DiffStatus } from "./ipc";

  /**
   * El informe de una comparación de datos y el SQL que igualaría el destino.
   *
   * Dos vistas de lo mismo, como en la comparación de esquemas. El script **no se ejecuta desde
   * acá**: se copia o se abre en una pestaña de consulta contra el destino.
   */
  let { tab }: { tab: DataCompareTab } = $props();

  let view = $state<"report" | "sql">("report");
  let copied = $state(false);

  const result = $derived(tab.result);
  const statements = $derived(result ? filterDataStatements(result.statements, tab.risks) : []);
  const script = $derived(dataScriptOf(statements));
  const riskCounts = $derived(countDataRisks(result?.statements ?? []));

  /** Qué clases de fila se ven. Es de la pantalla: no cambia lo que se compara ni el script. */
  let shown = $state<Record<DiffStatus, boolean>>({
    onlySource: true,
    onlyTarget: true,
    different: true,
  });
  const rows = $derived(result ? result.rows.filter((row) => shown[row.kind]) : []);

  const RISK_CHIPS = $derived([
    { key: "safe", label: t("Seguro"), tone: "text-emerald-700 dark:text-emerald-300", title: t("Los INSERT: agregan filas que faltan") },
    { key: "review", label: t("Para revisar"), tone: "text-amber-700 dark:text-amber-300", title: t("Los UPDATE: pisan valores del destino") },
    { key: "destructive", label: t("Destructivo"), tone: "text-rose-700 dark:text-rose-300", title: t("Los DELETE: borran filas que solo están en el destino") },
  ] as const);

  const MARK = $derived<Record<DiffStatus, { text: string; tone: string; title: string }>>({
    onlySource: { text: "+", tone: "tag-ok", title: t("Está en el origen y falta en el destino") },
    onlyTarget: { text: "−", tone: "tag-bad", title: t("Está en el destino y no en el origen") },
    different: { text: "~", tone: "tag-warn", title: t("Está en los dos, con valores distintos") },
  });

  /** Todas las columnas del informe: la clave primero, igual que en las filas. */
  const headers = $derived(result ? [...result.layout.keyColumns, ...result.layout.columns] : []);

  /** Los valores de una fila en el orden de `headers`. */
  function cells(key: string[], values: (string | null)[]): (string | null)[] {
    return [...key, ...values];
  }

  /** Lo que se dibuja por fila: una línea, o dos (origen y destino) cuando difiere. */
  function lines(row: DataRowDiff): { label: string; values: (string | null)[] }[] {
    if (row.kind === "different") {
      return [
        { label: t("origen"), values: cells(row.key, row.source) },
        { label: t("destino"), values: cells(row.key, row.target) },
      ];
    }
    return [{ label: "", values: cells(row.key, row.values) }];
  }

  const keyCount = $derived(result?.layout.keyColumns.length ?? 0);

  async function copy() {
    await navigator.clipboard.writeText(script).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  /** Abre el script en una consulta contra el **destino**: es donde hay que correrlo. */
  async function openInQuery() {
    const query = await openQuery(
      tab.target.id,
      tab.target.database,
      t("Sincronizar {table}", { table: tab.target.table }),
    );
    query.sql = script;
  }
</script>

<div class="flex h-full flex-col">
  <div class="toolbar divider-b gap-2">
    <div class="seg" role="tablist">
      <button
        class="seg-item"
        role="tab"
        aria-selected={view === "report"}
        onclick={() => (view = "report")}
      >
        {t("Diferencias")}
        {#if result}
          <span class="seg-count">
            {result.counts.onlySource + result.counts.onlyTarget + result.counts.different}
          </span>
        {/if}
      </button>
      <button
        class="seg-item"
        role="tab"
        aria-selected={view === "sql"}
        onclick={() => (view = "sql")}
      >
        SQL
        {#if result}<span class="seg-count">{statements.length}</span>{/if}
      </button>
    </div>

    <div class="flex-1"></div>

    {#if result}
      <span class="muted text-[11px] select-text">
        {result.source}
        <Icon name="chevron" size={11} class="mx-1 inline opacity-60" />
        {result.target}
      </span>
    {/if}

    {#if tab.loading}
      <button class="btn btn-ghost btn-sm" onclick={() => tab.cancel()}>
        <Icon name="close" size={13} />
        {t("Cancelar")}
      </button>
    {:else}
      <button
        class="btn btn-ghost btn-sm"
        title={t("Vuelve a leer las dos tablas y compara de nuevo")}
        onclick={() => tab.load()}
      >
        <Icon name="refresh" size={13} />
        {t("Comparar de nuevo")}
      </button>
    {/if}
  </div>

  {#if tab.loading}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 muted text-sm">
      <div class="flex items-center gap-2"><span class="spinner"></span> {t("Leyendo las dos tablas…")}</div>
      <div class="text-[11px]">
        {t("{source} filas del origen · {target} del destino", {
          source: tab.progress.sourceRows.toLocaleString(),
          target: tab.progress.targetRows.toLocaleString(),
        })}
      </div>
    </div>
  {:else if tab.error}
    <div class="p-3"><Alert tone="bad">{tab.error}</Alert></div>
  {:else if !result}
    <Empty icon="compare" title={t("Sin comparar")} hint={t("Todavía no se leyó ninguna tabla.")} />
  {:else if view === "report"}
    <div class="min-h-0 flex-1 overflow-auto">
      {#each result.warnings as warning (warning)}
        <Alert tone="warn" class="mx-3 mt-2">{warning}</Alert>
      {/each}

      {#if result.rows.length === 0}
        <Empty
          icon="check"
          title={t("No hay diferencias")}
          hint={t("Las {count} filas comparadas son iguales de los dos lados.", { count: result.counts.equal.toLocaleString() })}
        />
      {:else}
        <div class="flex flex-wrap items-center gap-2 px-3 py-2">
          {#each [["onlySource", t("solo en el origen"), result.counts.onlySource], ["onlyTarget", t("solo en el destino"), result.counts.onlyTarget], ["different", t("distintas"), result.counts.different]] as const as [status, label, total] (status)}
            <button
              class="chip-toggle"
              aria-pressed={shown[status]}
              title={MARK[status].title}
              onclick={() => (shown[status] = !shown[status])}
            >
              <span class="tag {MARK[status].tone} w-5 justify-center font-mono">{MARK[status].text}</span>
              {total.toLocaleString()} {label}
            </button>
          {/each}
          <span class="text-[11px] muted">{summary(result.counts)}</span>
        </div>

        <table class="w-full border-collapse font-mono text-[12px]">
          <thead>
            <tr class="divider-b text-left">
              <th class="w-6 px-2 py-1"></th>
              <th class="w-16 px-2 py-1"></th>
              {#each headers as header, index (index)}
                <th
                  class="px-2 py-1 font-medium {index < keyCount ? 'underline decoration-dotted' : ''}"
                  title={index < keyCount ? t("Columna de la clave") : ""}
                >
                  {header}
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each rows as row (row.kind + keyLabel(row.key))}
              {#each lines(row) as line, lineIndex (lineIndex)}
                <tr class={lineIndex === 0 ? "divider-t" : ""}>
                  <td class="px-2 py-0.5 align-top">
                    {#if lineIndex === 0}
                      <span
                        class="tag {MARK[row.kind].tone} w-5 justify-center"
                        title={MARK[row.kind].title}>{MARK[row.kind].text}</span
                      >
                    {/if}
                  </td>
                  <td class="muted px-2 py-0.5 text-[11px]">{line.label}</td>
                  {#each line.values as value, index (index)}
                    {@const cell = cellText(value)}
                    {@const changed = index >= keyCount && isChanged(row, index - keyCount)}
                    <td
                      class="max-w-[28rem] truncate px-2 py-0.5 select-text
                             {changed ? 'bg-amber-100 dark:bg-amber-900/30' : ''}"
                      title={cell.text}
                    >
                      {#if cell.isNull}<span class="cell-null">NULL</span>{:else}{cell.text}{/if}
                    </td>
                  {/each}
                </tr>
              {/each}
            {/each}
          </tbody>
        </table>

        {#if result.truncated}
          <p class="px-3 py-2 text-[11px] muted">
            {t("Se listan las primeras {count} diferencias.", { count: result.maxDiffs.toLocaleString() })}
          </p>
        {/if}
      {/if}
    </div>
  {:else}
    <div class="toolbar divider-b gap-2">
      {#each RISK_CHIPS as chip (chip.key)}
        <button
          class="chip-toggle"
          aria-pressed={tab.risks[chip.key]}
          title={chip.title}
          onclick={() => (tab.risks[chip.key] = !tab.risks[chip.key])}
        >
          <span class="font-medium {chip.tone}">{riskCounts[chip.key]}</span>
          {chip.label}
        </button>
      {/each}
      {#if !tab.risks.destructive && riskCounts.destructive > 0}
        <span class="text-[11px] text-rose-700 dark:text-rose-300">
          {t("{count} DELETE fuera del script", { count: riskCounts.destructive })}
        </span>
      {/if}

      <div class="flex-1"></div>

      <button class="btn btn-ghost btn-sm" disabled={!script} onclick={copy}>
        <Icon name={copied ? "check" : "copy"} size={13} />
        {copied ? t("Copiado") : t("Copiar")}
      </button>
      <button
        class="btn btn-primary btn-sm"
        disabled={!script}
        title={t("Abre el script en una consulta contra {database}, para revisarlo y correrlo", { database: tab.target.database })}
        onclick={openInQuery}
      >
        <Icon name="sql" size={13} />
        {t("Abrir en consulta")}
      </button>
    </div>

    <div class="min-h-0 flex-1 overflow-auto p-3">
      {#if result.truncated}
        <Alert tone="warn" box class="mb-3">
          {t("El script cubre solo las primeras {count} diferencias; hay más.", { count: result.maxDiffs.toLocaleString() })}
        </Alert>
      {/if}

      {#if script}
        <Sql code={script} />
      {:else}
        <Empty
          icon="check"
          title={t("Sin sentencias")}
          hint={result.statements.length === 0
            ? t("No hay nada que sincronizar.")
            : t("Los filtros de arriba dejaron fuera todas las sentencias.")}
        />
      {/if}
    </div>
  {/if}
</div>
