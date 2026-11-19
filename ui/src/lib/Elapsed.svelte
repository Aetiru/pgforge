<script lang="ts">
  import { runElapsed } from "./tx-view";

  /** Cuánto lleva corriendo algo: se redibuja solo, a décimas, mientras `since` no sea `null`. */
  let { since }: { since: number | null } = $props();

  let now = $state(Date.now());
  $effect(() => {
    if (since === null) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 100);
    return () => clearInterval(timer);
  });
</script>

{#if since !== null}<span class="tabular-nums">{runElapsed(since, now)}</span>{/if}
