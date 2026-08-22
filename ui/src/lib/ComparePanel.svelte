<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Sql from "./Sql.svelte";
  import { compareLabel, compareLook } from "./badges";
  import { countEntries, filterStatements, scriptOf } from "./compare-script";
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

  const diff = $derived(tab.result?.diff ?? null);
  const counts = $derived(diff ? countEntries(diff.entries) : null);
  const statements = $derived(
    tab.result ? filterStatements(tab.result.plan.statements, tab.risks) : [],
  );
  const script = $derived(scriptOf(statements));
  const warnings = $derived(tab.result?.plan.warnings ?? []);

  /** `+` falta en el destino, `−` sobra, `~` está en los dos y difiere. */
  const MARK: Record<DiffStatus, { text: string; tone: string; title: string }> = {
    onlySource: {
      text: "+",
      tone: "tag-ok",
      title: "Está en el origen y falta en el destino",
    },
    onlyTarget: {
      text: "−",
      tone: "tag-bad",
      title: "Está en el destino y no en el origen",
    },
    different: { text: "~", tone: "tag-warn", title: "Está en los dos, con diferencias" },
  };

  const DETAIL_LABEL = {
    column: "columna",
    constraint: "restricción",
    index: "índice",
    member: "valor",
    property: "propiedad",
  } as const;

  function toggle(entry: DiffEntry) {
    tab.opened = tab.opened === entry.name ? null : entry.name;
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
      `Sincronizar ${tab.target.schema}`,
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
        Diferencias
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
      title="Vuelve a leer los dos esquemas y compara de nuevo"
      disabled={tab.loading}
      onclick={() => tab.load()}
    >
      <Icon name="refresh" size={13} />
      Comparar de nuevo
    </button>
  </div>

  {#if tab.loading}
    <div class="flex flex-1 items-center justify-center gap-2 muted text-sm">
      <span class="spinner"></span> Leyendo los dos esquemas…
    </div>
  {:else if tab.error}
    <div class="p-3"><Alert tone="bad">{tab.error}</Alert></div>
  {:else if !diff || !tab.result}
    <Empty icon="compare" title="Sin comparar" hint="Todavía no se leyó ningún esquema." />
  {:else if view === "report"}
    <div class="min-h-0 flex-1 overflow-auto">
      {#if diff.entries.length === 0}
        <Empty
          icon="check"
          title="No hay diferencias"
          hint="Los {diff.equal} objetos comparados son iguales de los dos lados."
        />
      {:else}
        <div class="px-3 py-2 text-[11px] muted">
          {counts?.onlySource} solo en el origen · {counts?.onlyTarget} solo en el destino ·
          {counts?.different} distintos · {diff.equal} iguales
        </div>

        {#each diff.entries as entry (entry.kind + entry.name)}
          {@const look = compareLook(entry.kind)}
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
              <span class="muted text-[11px]">{compareLabel(entry.kind)}</span>
              <span class="flex-1"></span>
              {#if entry.details.length > 0}
                <span class="muted text-[11px]">{entry.details.length} diferencias</span>
              {/if}
              <Icon
                name="chevron"
                size={12}
                class="opacity-60 transition-transform {tab.opened === entry.name ? 'rotate-90' : ''}"
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

            {#if tab.opened === entry.name}
              <div class="grid gap-3 px-3 pb-3 pl-12 lg:grid-cols-2">
                <div class="card p-2">
                  <div class="label mb-1">Origen · {diff.source.schema}</div>
                  {#if entry.sourceDdl}
                    <Sql code={entry.sourceDdl} />
                  {:else}
                    <div class="muted text-[12px]">No existe de este lado.</div>
                  {/if}
                </div>
                <div class="card p-2">
                  <div class="label mb-1">Destino · {diff.target.schema}</div>
                  {#if entry.targetDdl}
                    <Sql code={entry.targetDdl} />
                  {:else}
                    <div class="muted text-[12px]">No existe de este lado.</div>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {:else}
    <div class="toolbar divider-b gap-3">
      <label class="check" title="Todo lo que solo agrega: no puede perder nada">
        <input type="checkbox" bind:checked={tab.risks.safe} />
        Seguro
      </label>
      <label class="check" title="Puede fallar o tardar contra una tabla con datos">
        <input type="checkbox" bind:checked={tab.risks.review} />
        Para revisar
      </label>
      <label class="check" title="Borra estructura, y con ella los datos que tenga adentro">
        <input type="checkbox" bind:checked={tab.risks.destructive} />
        Destructivo
      </label>

      <div class="flex-1"></div>

      <button class="btn btn-ghost btn-sm" disabled={!script} onclick={copy}>
        <Icon name={copied ? "check" : "copy"} size={13} />
        {copied ? "Copiado" : "Copiar"}
      </button>
      <button
        class="btn btn-primary btn-sm"
        disabled={!script}
        title="Abre el script en una consulta contra {tab.target.database}, para revisarlo y correrlo"
        onclick={openInQuery}
      >
        <Icon name="sql" size={13} />
        Abrir en consulta
      </button>
    </div>

    <div class="min-h-0 flex-1 overflow-auto p-3">
      {#if warnings.length > 0}
        <Alert tone="warn" box class="mb-3">
          <div class="font-medium">Esto no se puede resolver con un ALTER:</div>
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
          title="Sin sentencias"
          hint={tab.result.plan.statements.length === 0
            ? "No hay nada que sincronizar."
            : "Los filtros de arriba dejaron fuera todas las sentencias."}
        />
      {/if}
    </div>
  {/if}
</div>
