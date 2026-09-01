<script lang="ts">
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";
  import { font, FONT_LABELS, type FontChoice } from "./font.svelte";

  let { onclose }: { onclose: () => void } = $props();

  const OPTIONS: FontChoice[] = ["source-code-pro", "jetbrains-mono"];
</script>

<Modal title="Preferencias" subtitle="Vale para toda la aplicación" {onclose}>
  <div class="flex flex-col gap-1.5">
    <span class="label">Tipo de letra</span>
    <div class="grid grid-cols-2 gap-2">
      {#each OPTIONS as option (option)}
        <button
          class="card flex flex-col items-start gap-1 px-3 py-2.5 text-left transition-colors
            hover:border-blue-400 dark:hover:border-blue-500
            {font.choice === option ? 'border-blue-500 ring-2 ring-blue-500/20 dark:border-blue-500' : ''}"
          aria-pressed={font.choice === option}
          onclick={() => font.set(option)}
        >
          <span class="flex w-full items-center justify-between">
            <span class="text-sm font-medium">{FONT_LABELS[option]}</span>
            {#if font.choice === option}
              <Icon name="check" size={13} class="text-blue-600 dark:text-blue-400" />
            {/if}
          </span>
          <span
            class="select-text text-xs muted"
            style="font-family: {option === 'source-code-pro'
              ? '\'Source Code Pro\''
              : '\'JetBrains Mono\''}, ui-monospace, monospace"
          >
            SELECT * FROM tabla;
          </span>
        </button>
      {/each}
    </div>
  </div>
</Modal>
