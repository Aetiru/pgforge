<script lang="ts">
  import Icon from "./Icon.svelte";
  import { toasts } from "./toasts.svelte";
  import { view } from "./view.svelte";
</script>

<div class="pointer-events-none fixed right-4 bottom-4 z-50 flex flex-col gap-2">
  {#each toasts.items as item (item.id)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="alert-box pointer-events-auto w-72 cursor-pointer shadow-lg
             {item.tone === 'bad'
               ? 'border-rose-200 border-l-rose-500 bg-rose-50 text-rose-800 dark:border-rose-900 dark:border-l-rose-500 dark:bg-rose-950/90 dark:text-rose-200'
               : 'border-emerald-200 border-l-emerald-500 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:border-l-emerald-500 dark:bg-emerald-950/90 dark:text-emerald-200'}"
      onclick={() => {
        view.show("processes");
        toasts.dismiss(item.id);
      }}
    >
      <Icon name={item.tone === "bad" ? "warn" : "check"} size={14} class="mt-0.5 shrink-0" />
      <div class="min-w-0 flex-1">
        <p class="truncate text-sm font-medium">{item.title}</p>
        <p class="truncate text-xs opacity-80">{item.body}</p>
      </div>
      <button
        class="btn btn-ghost btn-icon shrink-0"
        aria-label="Cerrar"
        onclick={(event) => {
          event.stopPropagation();
          toasts.dismiss(item.id);
        }}
      >
        <Icon name="close" size={11} />
      </button>
    </div>
  {/each}
</div>
