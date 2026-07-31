<script lang="ts">
  import { appInfo, describeError, type AppInfo } from "./lib/ipc";

  let info = $state<AppInfo | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    appInfo()
      .then((value) => (info = value))
      .catch((err) => (error = describeError(err)));
  });
</script>

<main class="flex h-full flex-col items-center justify-center gap-3">
  <h1 class="text-3xl font-semibold tracking-tight">pgforge</h1>

  {#if error}
    <p class="text-sm text-red-600 dark:text-red-400">{error}</p>
  {:else if info}
    <p class="text-sm text-neutral-500 dark:text-neutral-400">
      v{info.version} · PostgreSQL {info.minPostgresMajor}+
    </p>
  {:else}
    <p class="text-sm text-neutral-400">Iniciando…</p>
  {/if}
</main>
