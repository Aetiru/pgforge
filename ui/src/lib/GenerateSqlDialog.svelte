<script lang="ts">
  import { t } from "./i18n.svelte";
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import Skeleton from "./Skeleton.svelte";
  import Sql from "./Sql.svelte";
  import { dataOpen, describeError, type TableShape } from "./ipc";
  import { openQuery } from "./query.svelte";
  import { GENERATE_KINDS, generateSql, type GenerateKind } from "./sql-generate";
  import type { GenerateTarget } from "./generate-sql.svelte";

  let { target, onclose }: { target: GenerateTarget; onclose: () => void } = $props();

  let kind = $state<GenerateKind>("select");
  let shape = $state<TableShape | null>(null);
  let error = $state<string | null>(null);
  let copied = $state(false);

  // La forma se lee una sola vez: describe el catálogo, no cambia mientras el diálogo está abierto.
  $effect(() => {
    const { profileId, oid, database } = untrack(() => target);
    dataOpen(profileId, oid, database)
      .then((loaded) => (shape = loaded))
      .catch((problem) => (error = describeError(problem)));
  });

  const sql = $derived(shape ? generateSql(shape, kind) : "");

  async function openInQuery() {
    const tab = await openQuery(target.profileId, target.database, target.title);
    tab.sql = sql;
    onclose();
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(sql);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Sin permiso del portapapeles el texto sigue a la vista para copiarlo a mano.
    }
  }
</script>

<Modal title={t("Generar SQL")} subtitle={target.title} size="lg" {onclose}>
  <div class="seg" role="tablist">
    {#each GENERATE_KINDS as item (item.value)}
      <button
        class="seg-item"
        role="tab"
        aria-selected={kind === item.value}
        onclick={() => (kind = item.value)}>{t(item.label)}</button
      >
    {/each}
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {:else if !shape}
    <div class="mt-3"><Skeleton rows={5} /></div>
  {:else}
    <div class="mt-3 max-h-80 overflow-auto rounded-md border border-zinc-200 p-2 dark:border-zinc-700 select-text">
      <Sql code={sql} />
    </div>
    <p class="mt-2 text-xs muted">
      {t("Lo que falta va como")} <code class="font-mono">{"{{variable}}"}</code>{t(": al ejecutar, la consulta pregunta cada valor.")}
    </p>
  {/if}

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>{t("Cerrar")}</button>
    <button class="btn" disabled={!shape} onclick={copy}>{copied ? t("Copiado ✓") : t("Copiar")}</button>
    <button class="btn btn-primary" disabled={!shape} onclick={openInQuery}>
      {t("Abrir en una consulta")}
    </button>
  {/snippet}
</Modal>
