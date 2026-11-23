<script lang="ts">
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import type { QueryTab } from "./query.svelte";
  import { shortcutStore } from "./shortcuts.svelte";
  import { txElapsed, txIsStale, txSummary } from "./tx-view";

  /**
   * La franja de la transacción abierta: existe solo mientras hay una. Dice lo que el riel no podía
   * —desde cuándo, cuánto se tocó— y trae Commit y Rollback con texto, que es justo el momento en
   * que se necesitan. Sin transacción no ocupa lugar.
   */
  let {
    tab,
    prodName = null,
  }: {
    tab: QueryTab;
    /** Nombre del servidor cuando es de producción: ahí la franja lo dice con palabras. */
    prodName?: string | null;
  } = $props();

  let now = $state(Date.now());
  $effect(() => {
    if (tab.txSince === null) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const failed = $derived(tab.txStatus === "failed");
  const stale = $derived(tab.txSince !== null && prodName !== null && txIsStale(tab.txSince, now));
  const alarm = $derived(failed || stale);
  const busy = $derived(tab.running || tab.tabId === null);
</script>

{#if tab.txStatus !== "idle"}
  <div
    class="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b px-3 py-1.5 text-xs
           {alarm
      ? 'border-rose-200 bg-rose-50 text-rose-800 dark:border-rose-500/30 dark:bg-rose-500/10 dark:text-rose-300'
      : 'border-amber-200 bg-amber-50 text-amber-800 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-300'}"
    role="status"
  >
    <Icon name={failed ? "warn" : "edit"} size={13} />
    <span class="font-semibold">
      {#if prodName}{t("PRODUCCIÓN")} · {prodName} · {/if}{failed ? t("Transacción abortada") : t("Transacción abierta")}
    </span>

    <span class="min-w-0 flex-1">
      {#if failed}
        {t("Una sentencia falló: el servidor rechaza todo lo que siga hasta que hagas Rollback.")}
      {:else}
        {txSummary(tab.txStatements, tab.txRows)}
        {#if tab.txSince !== null}· {t("hace {time}", { time: txElapsed(tab.txSince, now) })}{/if}
        {#if stale}
          · {t("la sesión retiene candados mientras no la cierres")}
        {/if}
      {/if}
    </span>

    <button
      class="btn btn-sm btn-danger-ghost"
      disabled={busy}
      title={shortcutStore.hint(t("Descarta todo lo hecho desde que se abrió la transacción"), "editor.rollback")}
      onclick={() => tab.rollback()}
    >
      <Icon name="undo" size={12} />
      Rollback
    </button>
    {#if !failed}
      <button
        class="btn btn-sm btn-primary"
        disabled={busy}
        title={shortcutStore.hint(t("Confirma la transacción abierta en esta pestaña"), "editor.commit")}
        onclick={() => tab.commit()}
      >
        <Icon name="check" size={12} />
        Commit
      </button>
    {/if}
  </div>
{/if}
