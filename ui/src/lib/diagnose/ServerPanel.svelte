<script lang="ts">
  import { untrack } from "svelte";
  import { t } from "../i18n.svelte";
  import Alert from "../Alert.svelte";
  import Empty from "../Empty.svelte";
  import Icon from "../Icon.svelte";
  import { ago, bytes, count, duration, percent } from "../format";
  import {
    checkpointStats,
    describeError,
    serverConfigReview,
    type CheckpointReport,
    type ConfigReview,
  } from "../ipc";
  import { levelLabel, levelTag } from "../diagnose-view";
  import { openQuery } from "../query.svelte";
  import { Loader } from "./loader.svelte";
  import PanelFrame from "./PanelFrame.svelte";
  import { loadRam, saveRam } from "./ram";

  /**
   * Cómo trabaja el servidor y qué parámetros revisar. No cambia nada: cada hallazgo trae la
   * sentencia para copiar o abrir en una consulta, igual que las sugerencias de índices.
   */
  let { profileId, database }: { profileId: string; database: string | null } = $props();

  const checkpoints = new Loader<CheckpointReport>();
  const config = new Loader<ConfigReview>();

  // La RAM la dice el usuario: el servidor no sabe cuánta memoria tiene la máquina, y sin ella no se
  // sugiere ningún tamaño en vez de adivinar uno.
  let ramGb = $state<string>(untrack(() => loadRam(profileId)));
  const ramBytes = $derived.by(() => {
    const gb = Number(ramGb.replace(",", "."));
    return Number.isFinite(gb) && gb > 0 ? Math.round(gb * 1024 ** 3) : null;
  });

  function refresh() {
    void checkpoints.load(() => checkpointStats(profileId));
    void config.load(() => serverConfigReview(profileId, ramBytes));
  }

  $effect(() => {
    void database;
    void profileId;
    untrack(() => {
      checkpoints.reset();
      config.reset();
      refresh();
    });
  });

  function applyRam() {
    saveRam(profileId, ramGb);
    void config.load(() => serverConfigReview(profileId, ramBytes));
  }

  let message = $state<string | null>(null);

  async function copy(sql: string) {
    try {
      await navigator.clipboard.writeText(sql);
      message = t("Sentencia copiada.");
    } catch {
      message = t("No se pudo copiar: seleccionala y copiala a mano.");
    }
  }

  async function open(sql: string) {
    try {
      const tab = await openQuery(profileId, database ?? "", t("Configuración"));
      tab.sql = `${sql};\n${t("-- Para que valga sin reiniciar (si el parámetro lo permite):")}\n-- SELECT pg_reload_conf();`;
    } catch (error) {
      message = describeError(error);
    }
  }
</script>

<PanelFrame
  title={t("Servidor")}
  icon="sliders"
  loading={checkpoints.loading || config.loading}
  loadedAt={checkpoints.loadedAt ?? config.loadedAt}
  error={checkpoints.error ?? config.error}
  onrefresh={refresh}
>
  <div class="flex flex-col gap-5 p-3">
    {#if message}<Alert tone="ok">{message}</Alert>{/if}

    <section>
      <h3 class="mb-2 flex items-center gap-2 text-xs font-semibold">
        {t("Checkpoints y WAL")}
        {#if checkpoints.data}
          <span class="tag {levelTag(checkpoints.data.level)}">{levelLabel(checkpoints.data.level)}</span>
        {/if}
      </h3>
      {#if checkpoints.data}
        {@const c = checkpoints.data}
        <div class="grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
          <div class="rounded border border-zinc-200 p-2 dark:border-zinc-700">
            <div class="muted">{t("Programados")}</div>
            <div class="text-base font-semibold">{count(c.timed)}</div>
          </div>
          <div class="rounded border border-zinc-200 p-2 dark:border-zinc-700">
            <div class="muted">{t("Forzados")}</div>
            <div class="text-base font-semibold">
              {count(c.requested)}
              {#if c.forcedRatio !== null}<span class="text-xs font-normal muted">({percent(c.forcedRatio, 0)})</span>{/if}
            </div>
          </div>
          <div class="rounded border border-zinc-200 p-2 dark:border-zinc-700">
            <div class="muted">{t("Escribiendo / sincronizando")}</div>
            <div class="text-base font-semibold">{duration(c.writeSeconds)} / {duration(c.syncSeconds)}</div>
          </div>
          <div class="rounded border border-zinc-200 p-2 dark:border-zinc-700">
            <div class="muted">{t("WAL promedio")}</div>
            <div class="text-base font-semibold">
              {c.wal?.bytesPerSecond != null ? `${bytes(c.wal.bytesPerSecond)}/s` : "—"}
            </div>
            {#if !c.wal}<div class="muted">{t("PostgreSQL 13 no lo informa")}</div>{/if}
          </div>
        </div>
        <p class="mt-2 text-xs muted">
          max_wal_size {c.maxWalSizeBytes !== null ? bytes(c.maxWalSizeBytes) : "—"} ·
          checkpoint_timeout {c.checkpointTimeoutSeconds !== null ? duration(c.checkpointTimeoutSeconds) : "—"} ·
          {t("contando {since}", { since: c.resetSeconds !== null ? ago(c.resetSeconds) : t("desde el arranque") })}
        </p>
        {#each c.advice as line (line)}
          <p class="mt-1 text-xs">{line}</p>
        {/each}
      {:else if !checkpoints.error}
        <Empty icon="gauge" title={t("Leyendo checkpoints…")} />
      {/if}
    </section>

    <section>
      <h3 class="mb-1 text-xs font-semibold">{t("Revisión de configuración")}</h3>
      <div class="mb-3 flex flex-wrap items-center gap-2 text-xs">
        <label class="flex items-center gap-1.5" for="ram-gb">
          {t("RAM del servidor")}
          <input
            id="ram-gb"
            class="field w-20 text-right"
            inputmode="decimal"
            placeholder="16"
            bind:value={ramGb}
            onkeydown={(event) => event.key === "Enter" && applyRam()}
          />
          GB
        </label>
        <button class="btn btn-sm" onclick={applyRam}>{t("Aplicar")}</button>
        <span class="muted">
          {t("El servidor no sabe cuánta memoria tiene la máquina. Sin este dato no se sugieren tamaños.")}
        </span>
      </div>
      {#if config.data}
        {@const review = config.data}
        {#if review.findings.length === 0}
          <Empty icon="check" title={t("Nada para revisar")} hint={t("Los parámetros que se miran están en valores razonables.")} />
        {/if}
        <ul class="flex flex-col gap-2">
          {#each review.findings as f (f.name)}
            <li class="rounded-lg border border-zinc-200 p-3 text-xs dark:border-zinc-700">
              <div class="flex flex-wrap items-center gap-2">
                <span class="font-mono text-sm font-semibold">{f.name}</span>
                <span class="tag {levelTag(f.level)}">{levelLabel(f.level)}</span>
                {#if f.restart}<span class="tag tag-neutral">{t("pide reiniciar")}</span>{/if}
                <span class="ml-auto font-mono">
                  {f.current}{#if f.suggested} → {f.suggested}{/if}
                </span>
              </div>
              <p class="mt-1.5">{f.reason}</p>
              {#if f.alterSql}
                <div class="mt-2 flex flex-wrap items-center gap-2">
                  <code class="select-text rounded bg-zinc-100 px-2 py-1 font-mono dark:bg-zinc-800">{f.alterSql}</code>
                  <button class="btn btn-sm" onclick={() => copy(f.alterSql!)}>
                    <Icon name="copy" size={12} /> {t("Copiar")}
                  </button>
                  <button class="btn btn-sm" onclick={() => open(f.alterSql!)}>
                    <Icon name="play" size={12} /> {t("Abrir en una consulta")}
                  </button>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
        {#if review.hidden.length}
          <p class="mt-2 text-xs muted">
            {t("Este rol no puede ver: {names} (hace falta {setting}).", { names: review.hidden.join(", "), setting: "pg_read_all_settings" })}
          </p>
        {/if}
      {:else if !config.error}
        <Empty icon="sliders" title={t("Leyendo la configuración…")} />
      {/if}
    </section>
  </div>
</PanelFrame>
