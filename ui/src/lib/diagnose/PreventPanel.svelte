<script lang="ts">
  import { untrack } from "svelte";
  import DataGrid, { type Column } from "../DataGrid.svelte";
  import Empty from "../Empty.svelte";
  import { ago, bytes, count, percent } from "../format";
  import {
    autovacuumStatus,
    sequenceUsage,
    wraparoundStatus,
    type AutovacuumRow,
    type SequenceUsage,
    type Wraparound,
  } from "../ipc";
  import { daysText, levelLabel, levelTag, millions } from "../diagnose-view";
  import { Loader } from "./loader.svelte";
  import PanelFrame from "./PanelFrame.svelte";

  /**
   * Lo que no duele hasta el día en que para la base: secuencias que se agotan, transacciones que
   * se acercan al límite de 32 bits y tablas a las que el autovacuum no llega.
   */
  let { profileId, database }: { profileId: string; database: string | null } = $props();

  const sequences = new Loader<SequenceUsage[]>();
  const wrap = new Loader<Wraparound>();
  const vacuum = new Loader<AutovacuumRow[]>();

  function refresh() {
    void sequences.load(() => sequenceUsage(profileId));
    void wrap.load(() => wraparoundStatus(profileId));
    void vacuum.load(() => autovacuumStatus(profileId));
  }

  $effect(() => {
    void database;
    void profileId;
    untrack(() => {
      sequences.reset();
      wrap.reset();
      vacuum.reset();
      refresh();
    });
  });

  const loading = $derived(sequences.loading || wrap.loading || vacuum.loading);
  const loadedAt = $derived(
    Math.max(sequences.loadedAt ?? 0, wrap.loadedAt ?? 0, vacuum.loadedAt ?? 0) || null,
  );
  const error = $derived(sequences.error ?? wrap.error ?? vacuum.error);

  const seqColumns: Column<SequenceUsage>[] = [
    { key: "name", header: "Secuencia", width: 260, value: (s) => `${s.schema}.${s.name}` },
    { key: "type", header: "Tipo", width: 90, value: (s) => s.dataType },
    {
      key: "used",
      header: "Usado",
      width: 90,
      align: "right",
      value: (s) => percent(s.usedRatio),
      sort: (s) => s.usedRatio,
      tone: (s) => (s.level === "attention" ? "text-rose-600" : s.level === "review" ? "text-amber-600 dark:text-amber-400" : undefined),
    },
    {
      key: "remaining",
      header: "Quedan",
      width: 140,
      align: "right",
      value: (s) => count(s.remaining),
      sort: (s) => s.remaining,
    },
    {
      key: "perDay",
      header: "Ritmo",
      width: 130,
      align: "right",
      value: (s) => (s.perDay === null ? "faltan muestras" : `${count(Math.round(s.perDay))}/día`),
      sort: (s) => s.perDay ?? -1,
    },
    {
      key: "left",
      header: "Se agota en",
      width: 130,
      align: "right",
      value: (s) => daysText(s.daysLeft),
      sort: (s) => s.daysLeft ?? Number.MAX_SAFE_INTEGER,
    },
    { key: "level", header: "Estado", width: 130, value: (s) => levelLabel(s.level) },
  ];

  const vacuumColumns: Column<AutovacuumRow>[] = [
    { key: "table", header: "Tabla", width: 260, value: (r) => `${r.schema}.${r.table}` },
    {
      key: "dead",
      header: "Filas muertas",
      width: 120,
      align: "right",
      value: (r) => count(r.deadTuples),
      sort: (r) => r.deadTuples,
    },
    {
      key: "at",
      header: "Umbral",
      width: 110,
      align: "right",
      value: (r) => count(Math.round(r.vacuumAt)),
      sort: (r) => r.vacuumAt,
      title: (r) =>
        r.ownSettings ? "La tabla tiene sus propios parámetros de autovacuum" : "Parámetros del servidor",
    },
    {
      key: "missing",
      header: "Faltan",
      width: 110,
      align: "right",
      value: (r) => (r.status === "due" ? "ya le toca" : count(r.vacuumMissing)),
      sort: (r) => r.vacuumMissing,
    },
    {
      key: "last",
      header: "Última limpieza",
      width: 140,
      value: (r) => ago(r.lastAutovacuumSeconds),
      sort: (r) => r.lastAutovacuumSeconds ?? Number.MAX_SAFE_INTEGER,
    },
    {
      key: "size",
      header: "Tamaño",
      width: 100,
      align: "right",
      value: (r) => bytes(r.totalBytes),
      sort: (r) => r.totalBytes,
    },
    {
      key: "status",
      header: "Estado",
      width: 150,
      value: (r) =>
        r.status === "disabled" ? "apagado" : r.level === "review" ? "le toca y no corre" : r.status === "due" ? "le toca" : "en espera",
      tone: (r) => (r.level === "review" ? "text-amber-600 dark:text-amber-400" : undefined),
    },
  ];

  const stuck = $derived((vacuum.data ?? []).filter((r) => r.level !== "ok").length);
</script>

<PanelFrame
  title="Prevención"
  icon="warn"
  {loading}
  {loadedAt}
  {error}
  onrefresh={refresh}
>
  <div class="flex flex-col gap-4 p-3">
    <section>
      <h3 class="mb-1 text-xs font-semibold">Secuencias por agotarse</h3>
      <p class="mb-2 text-xs muted">
        Una columna <span class="font-mono">serial</span> es un <span class="font-mono">integer</span>
        y llega a 2.147.483.647. El ritmo sale de las muestras que se guardan cada vez que se abre
        esta pestaña (como mucho una por hora).
      </p>
      <div class="h-56">
        <DataGrid
          columns={seqColumns}
          rows={sequences.data ?? []}
          rowKey={(s) => `${s.schema}.${s.name}`}
          sortable
          empty="Esta base no tiene secuencias."
        />
      </div>
    </section>

    <section>
      <h3 class="mb-1 text-xs font-semibold">Edad de las transacciones</h3>
      {#if wrap.data}
        {@const w = wrap.data}
        <p class="mb-2 text-xs muted">
          El autovacuum fuerza una limpieza a los {millions(w.freezeMaxAge)}. A los 2.147 M el
          servidor deja de aceptar escrituras.
        </p>
        <div class="flex flex-col gap-1.5">
          {#each w.databases as db (db.name)}
            <div class="flex items-center gap-2 text-xs">
              <span class="w-40 truncate font-mono">{db.name}</span>
              <div class="h-2 flex-1 rounded bg-zinc-200 dark:bg-zinc-700" title={`${db.age.toLocaleString("es")} transacciones`}>
                <div
                  class="h-2 rounded {db.level === 'attention' ? 'bg-rose-500' : db.level === 'review' ? 'bg-amber-500' : 'bg-emerald-500'}"
                  style:width="{Math.max(1, Math.min(100, db.ratio * 100))}%"
                ></div>
              </div>
              <span class="w-20 text-right font-mono">{millions(db.age)}</span>
              <span class="tag {levelTag(db.level)}">{levelLabel(db.level)}</span>
            </div>
          {/each}
        </div>
        {#if w.tables.some((t) => t.level !== "ok")}
          <p class="mt-2 text-xs muted">Las tablas que más la sostienen:</p>
          <ul class="mt-1 text-xs">
            {#each w.tables.filter((t) => t.level !== "ok") as t (`${t.schema}.${t.table}`)}
              <li class="font-mono">{t.schema}.{t.table} · {millions(t.age)} · {bytes(t.totalBytes)}</li>
            {/each}
          </ul>
        {/if}
      {:else}
        <Empty icon="info" title="Leyendo la edad de las transacciones…" />
      {/if}
    </section>

    <section>
      <h3 class="mb-1 text-xs font-semibold">
        Cuándo le toca el autovacuum a cada tabla
        {#if stuck > 0}<span class="tag tag-warn ml-1">{stuck}</span>{/if}
      </h3>
      <p class="mb-2 text-xs muted">
        El umbral es <span class="font-mono">umbral + factor × filas</span>, con los parámetros de
        la tabla si los tiene. «Le toca y no corre» es una tabla que pasó el umbral y lleva más de
        un día sin limpiarse: suele ser una transacción vieja que retiene el horizonte.
      </p>
      <div class="h-64">
        <DataGrid
          columns={vacuumColumns}
          rows={vacuum.data ?? []}
          rowKey={(r) => `${r.schema}.${r.table}`}
          sortable
          empty="No hay tablas con estadísticas."
        />
      </div>
    </section>
  </div>
</PanelFrame>
