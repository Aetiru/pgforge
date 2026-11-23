<script lang="ts">
  import { t } from "./i18n.svelte";
  import Skeleton from "./Skeleton.svelte";
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import MigrationSaveDialog from "./MigrationSaveDialog.svelte";
  import Sql from "./Sql.svelte";
  import {
    COMPARE_CATEGORIES,
    compareKindLabel,
    compareKindLook,
    countCategories,
    filterByCategory,
    type CompareCategory,
  } from "./compare-kinds";
  import { countEntries, countRisks, filterStatements, scriptOf } from "./compare-script";
  import { openQuery } from "./query.svelte";
  import type { CompareTab } from "./compare.svelte";
  import type { DiffDetail, DiffEntry, DiffStatus } from "./ipc";

  /**
   * El informe de una comparación y el SQL que la resolvería.
   *
   * Dos vistas de lo mismo y no dos pestañas: se mira el informe para entender qué cambió y se pasa
   * al SQL para llevárselo. El script **no se ejecuta desde acá** —se copia o se abre en una pestaña
   * de consulta contra el destino—, que es lo que permite leerlo antes de correrlo.
   */
  let { tab }: { tab: CompareTab } = $props();

  let view = $state<"report" | "sql">("report");
  let copied = $state(false);
  let savingMigration = $state(false);

  const diff = $derived(tab.result?.diff ?? null);
  const counts = $derived(diff ? countEntries(diff.entries) : null);
  const statements = $derived(
    tab.result ? filterStatements(tab.result.plan.statements, tab.risks) : [],
  );
  const script = $derived(scriptOf(statements));
  const riskCounts = $derived(countRisks(tab.result?.plan.statements ?? []));

  /** Qué estados del informe se ven. Es de la pantalla: no cambia lo que se compara ni el script. */
  let shown = $state<Record<DiffStatus, boolean>>({
    onlySource: true,
    onlyTarget: true,
    different: true,
  });
  /**
   * Y qué categorías: estructura, lógica y acceso. Con las funciones y los permisos en el mismo
   * informe, una diferencia de estructura se pierde entre cincuenta de permisos; así se miran por
   * separado. Tampoco toca el script —eso lo filtra el riesgo—.
   */
  let categories = $state<Record<CompareCategory, boolean>>({
    structure: true,
    logic: true,
    access: true,
  });
  const categoryCounts = $derived(diff ? countCategories(diff.entries) : null);
  const entries = $derived(
    diff ? filterByCategory(diff.entries, categories).filter((entry) => shown[entry.status]) : [],
  );

  const RISK_CHIPS = $derived([
    { key: "safe", label: t("Seguro"), tone: "text-emerald-700 dark:text-emerald-300", title: t("Todo lo que solo agrega: no puede perder nada") },
    { key: "review", label: t("Para revisar"), tone: "text-amber-700 dark:text-amber-300", title: t("Puede fallar o tardar contra una tabla con datos") },
    { key: "destructive", label: t("Destructivo"), tone: "text-rose-700 dark:text-rose-300", title: t("Borra estructura, y con ella los datos que tenga adentro") },
  ] as const);
  const warnings = $derived(tab.result?.plan.warnings ?? []);

  /** `+` falta en el destino, `−` sobra, `~` está en los dos y difiere. */
  const MARK = $derived<Record<DiffStatus, { text: string; tone: string; title: string }>>({
    onlySource: {
      text: "+",
      tone: "tag-ok",
      title: t("Está en el origen y falta en el destino"),
    },
    onlyTarget: {
      text: "−",
      tone: "tag-bad",
      title: t("Está en el destino y no en el origen"),
    },
    different: { text: "~", tone: "tag-warn", title: t("Está en los dos, con diferencias") },
  });

  const DETAIL_LABEL = $derived({
    column: t("columna"),
    constraint: t("restricción"),
    index: t("índice"),
    member: t("valor"),
    privilege: t("permiso"),
    property: t("propiedad"),
  } as const);

  /**
   * Con qué se identifica una fila abierta. El nombre solo no alcanza: una tabla y sus permisos o su
   * seguridad por filas se llaman igual y se abrirían juntas.
   */
  const rowKey = (entry: DiffEntry) => `${entry.kind}:${entry.name}`;

  function toggle(entry: DiffEntry) {
    tab.opened = tab.opened === rowKey(entry) ? null : rowKey(entry);
  }

  /**
   * El texto que resume una diferencia adentro de un objeto, en una línea. Vacío cuando repetiría
   * el nombre: el valor de una enumeración es su propio texto.
   */
  function detailText(detail: DiffDetail): string {
    if (detail.source && detail.target) return `${detail.source}  ≠  ${detail.target}`;
    const text = detail.source ?? detail.target ?? "";
    return text === detail.name ? "" : text;
  }

  async function copy() {
    await navigator.clipboard.writeText(script).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  /** Abre el script en una pestaña de consulta contra el **destino**: es donde hay que correrlo. */
  async function openInQuery() {
    const query = await openQuery(
      tab.target.id,
      tab.target.database,
      t("Sincronizar {schema}", { schema: tab.target.schema }),
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
        {#if diff}<span class="seg-count">{diff.entries.length}</span>{/if}
      </button>
      <button
        class="seg-item"
        role="tab"
        aria-selected={view === "sql"}
        onclick={() => (view = "sql")}
      >
        SQL
        {#if tab.result}<span class="seg-count">{statements.length}</span>{/if}
      </button>
    </div>

    <div class="flex-1"></div>

    {#if diff}
      <span class="muted text-[11px] select-text">
        {diff.source.server} · {diff.source.database}.{diff.source.schema}
        <span class="opacity-60">(PostgreSQL {diff.source.version})</span>
        <Icon name="chevron" size={11} class="mx-1 inline opacity-60" />
        {diff.target.server} · {diff.target.database}.{diff.target.schema}
        <span class="opacity-60">(PostgreSQL {diff.target.version})</span>
      </span>
    {/if}

    <button
      class="btn btn-ghost btn-sm"
      title={t("Vuelve a leer los dos esquemas y compara de nuevo")}
      disabled={tab.loading}
      onclick={() => tab.load()}
    >
      <Icon name="refresh" size={13} />
      {t("Comparar de nuevo")}
    </button>
  </div>

  {#if tab.loading}
    <div class="flex-1 overflow-hidden">
      <p class="px-4 pt-4 text-sm muted">{t("Leyendo los dos esquemas…")}</p>
      <Skeleton rows={8} label={t("Leyendo los dos esquemas…")} />
    </div>
  {:else if tab.error}
    <div class="p-3"><Alert tone="bad">{tab.error}</Alert></div>
  {:else if !diff || !tab.result}
    <Empty icon="compare" title={t("Sin comparar")} hint={t("Todavía no se leyó ningún esquema.")} />
  {:else if view === "report"}
    <div class="min-h-0 flex-1 overflow-auto">
      {#if diff.entries.length === 0}
        <Empty
          icon="check"
          title={t("No hay diferencias")}
          hint={t("Los {n} objetos comparados son iguales de los dos lados.", { n: diff.equal })}
        />
      {:else}
        <!-- Los tres estados son filtros: en un esquema con doscientas diferencias, lo que falta en
             el destino se mira solo y no mezclado con lo que difiere. -->
        <div class="flex flex-wrap items-center gap-2 px-3 py-2">
          {#each [["onlySource", t("solo en el origen"), counts?.onlySource], ["onlyTarget", t("solo en el destino"), counts?.onlyTarget], ["different", t("distintos"), counts?.different]] as const as [status, label, total] (status)}
            <button
              class="chip-toggle"
              aria-pressed={shown[status]}
              title={MARK[status].title}
              onclick={() => (shown[status] = !shown[status])}
            >
              <span class="tag {MARK[status].tone} w-5 justify-center font-mono">{MARK[status].text}</span>
              {total} {label}
            </button>
          {/each}
          <span class="text-[11px] muted">{t("{n} iguales", { n: diff.equal })}</span>
        </div>

        <!-- Las categorías: solo las que tienen algo, para no ofrecer un filtro que no filtra. -->
        <div class="flex flex-wrap items-center gap-2 px-3 pb-2">
          {#each COMPARE_CATEGORIES.filter((c) => (categoryCounts?.[c.key] ?? 0) > 0) as category (category.key)}
            <button
              class="chip-toggle"
              aria-pressed={categories[category.key]}
              title={t(category.title)}
              onclick={() => (categories[category.key] = !categories[category.key])}
            >
              {categoryCounts?.[category.key]}
              {t(category.label)}
            </button>
          {/each}
        </div>

        {#each entries as entry (entry.kind + entry.name)}
          {@const look = compareKindLook(entry.kind)}
          <div class="divider-b">
            <button
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-zinc-100
                     dark:hover:bg-zinc-700/60"
              onclick={() => toggle(entry)}
            >
              <span class="tag {MARK[entry.status].tone} w-5 justify-center font-mono"
                    title={MARK[entry.status].title}>{MARK[entry.status].text}</span>
              <Icon name={look.icon} size={13} class={look.tone} />
              <span class="text-sm select-text">{entry.name}</span>
              <span class="muted text-[11px]">{t(compareKindLabel(entry.kind))}</span>
              <span class="flex-1"></span>
              {#if entry.details.length > 0}
                <span class="muted text-[11px]">
                  {entry.details.length === 1
                    ? t("{n} diferencia", { n: entry.details.length })
                    : t("{n} diferencias", { n: entry.details.length })}
                </span>
              {/if}
              <Icon
                name="chevron"
                size={12}
                class="opacity-60 transition-transform {tab.opened === rowKey(entry) ? 'rotate-90' : ''}"
              />
            </button>

            {#if entry.details.length > 0}
              <div class="px-3 pb-1.5 pl-12">
                {#each entry.details as detail (detail.kind + detail.name)}
                  <div class="flex items-baseline gap-2 py-0.5 text-[12px]">
                    <span class="tag {MARK[detail.status].tone} w-5 justify-center font-mono"
                          title={MARK[detail.status].title}>{MARK[detail.status].text}</span>
                    <span class="muted w-20 shrink-0 text-[11px]">{DETAIL_LABEL[detail.kind]}</span>
                    <span class="shrink-0 select-text">{detail.name}</span>
                    <span class="min-w-0 flex-1 truncate muted select-text" title={detailText(detail)}>
                      {detailText(detail)}
                    </span>
                  </div>
                {/each}
              </div>
            {/if}

            {#if tab.opened === rowKey(entry)}
              <div class="grid gap-3 px-3 pb-3 pl-12 lg:grid-cols-2">
                <div class="card p-2">
                  <div class="label mb-1">{t("Origen · {schema}", { schema: diff.source.schema })}</div>
                  {#if entry.sourceDdl}
                    <Sql code={entry.sourceDdl} />
                  {:else}
                    <div class="muted text-[12px]">{t("No existe de este lado.")}</div>
                  {/if}
                </div>
                <div class="card p-2">
                  <div class="label mb-1">{t("Destino · {schema}", { schema: diff.target.schema })}</div>
                  {#if entry.targetDdl}
                    <Sql code={entry.targetDdl} />
                  {:else}
                    <div class="muted text-[12px]">{t("No existe de este lado.")}</div>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {/each}
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
          {riskCounts.destructive === 1
            ? t("{n} destructiva fuera del script", { n: riskCounts.destructive })
            : t("{n} destructivas fuera del script", { n: riskCounts.destructive })}
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
      <button
        class="btn btn-sm"
        disabled={!script}
        title={t("Guarda el script como archivo de migración numerado (V012__…sql)")}
        onclick={() => (savingMigration = true)}
      >
        <Icon name="save" size={13} />
        {t("Guardar como migración")}
      </button>
    </div>

    <div class="min-h-0 flex-1 overflow-auto p-3">
      {#if warnings.length > 0}
        <Alert tone="warn" box class="mb-3">
          <div class="font-medium">{t("Esto no se puede resolver con un ALTER:")}</div>
          <ul class="mt-1 list-disc pl-4">
            {#each warnings as warning (warning)}
              <li>{warning}</li>
            {/each}
          </ul>
        </Alert>
      {/if}

      {#if script}
        <Sql code={script} />
      {:else}
        <Empty
          icon="check"
          title={t("Sin sentencias")}
          hint={tab.result.plan.statements.length === 0
            ? t("No hay nada que sincronizar.")
            : t("Los filtros de arriba dejaron fuera todas las sentencias.")}
        />
      {/if}
    </div>
  {/if}
</div>

{#if savingMigration}
  <MigrationSaveDialog
    sql={script}
    profileId={tab.target.id}
    database={tab.target.database}
    schema={tab.target.schema}
    onclose={() => (savingMigration = false)}
  />
{/if}
