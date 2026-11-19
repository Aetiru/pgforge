<script lang="ts">
  import type { Snippet } from "svelte";
  import Alert from "../Alert.svelte";
  import Icon, { type IconName } from "../Icon.svelte";
  import { ago } from "../format";

  /**
   * El marco de cada pestaña de diagnóstico: título, «Actualizado hace …» y el botón de releer.
   * Estas estadísticas no se refrescan solas —cada lectura recorre el catálogo— y la hora de la
   * última dice cuánto confiar en lo que se ve.
   */
  let {
    title,
    icon,
    loading,
    loadedAt,
    error = null,
    onrefresh,
    actions = undefined,
    children,
  }: {
    title: string;
    icon: IconName;
    loading: boolean;
    loadedAt: number | null;
    error?: string | null;
    onrefresh: () => void;
    actions?: Snippet;
    children: Snippet;
  } = $props();

  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
</script>

<div class="flex min-h-0 flex-1 flex-col bg-zinc-50/70 px-4 py-4 dark:bg-black/15">
  <div class="card flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl shadow-sm">
    <div class="card-head justify-between">
      <span class="card-title flex items-center gap-1.5">
        <Icon name={icon} size={13} />
        {title}
      </span>
      <div class="flex items-center gap-2">
        <span class="text-[11px] muted" aria-live="polite">
          {#if loading}
            Actualizando…
          {:else if loadedAt}
            {@const seconds = Math.max(0, Math.round((now - loadedAt) / 1000))}
            {seconds < 5 ? "Actualizado ahora" : `Actualizado ${ago(seconds)}`}
          {/if}
        </span>
        {#if actions}{@render actions()}{/if}
        <button class="btn btn-sm" disabled={loading} onclick={onrefresh}>
          {#if loading}<span class="spinner"></span>{:else}<Icon name="refresh" size={12} />{/if}
          Actualizar
        </button>
      </div>
    </div>
    {#if error}
      <Alert tone="bad">{error}</Alert>
    {/if}
    <div class="min-h-0 flex-1 overflow-auto">
      {@render children()}
    </div>
  </div>
</div>
