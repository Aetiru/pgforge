<script lang="ts">
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";
  import { font, FONT_LABELS, type FontChoice } from "./font.svelte";
  import SqlColors from "./SqlColors.svelte";
  import { theme, type ThemePreference } from "./theme.svelte";

  let { onclose, onshortcuts }: { onclose: () => void; onshortcuts?: () => void } = $props();

  const OPTIONS: FontChoice[] = ["source-code-pro", "jetbrains-mono"];

  const THEMES: { id: ThemePreference; label: string }[] = [
    { id: "system", label: "Sistema" },
    { id: "light", label: "Claro" },
    { id: "dark", label: "Oscuro" },
    { id: "paper", label: "Papel" },
  ];
</script>

<Modal title="Preferencias" subtitle="Vale para toda la aplicación" size="lg" {onclose}>
  <div class="flex flex-col gap-1.5">
    <span class="label">Tema</span>
    <div class="seg self-start" role="group" aria-label="Tema">
      {#each THEMES as option (option.id)}
        <button
          class="seg-item"
          aria-pressed={theme.preference === option.id}
          onclick={() => theme.set(option.id)}
        >
          {option.label}
        </button>
      {/each}
    </div>
    <p class="text-xs muted">
      «Papel» es un claro de fondos crema y acento verde azulado, más cálido que el claro común.
    </p>
  </div>

  <div class="divider-t my-4"></div>

  <div class="flex flex-col gap-1.5">
    <span class="label">Tipo de letra del código y los datos</span>
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

  <div class="divider-t my-4"></div>

  <SqlColors />

  {#if onshortcuts}
    <div class="divider-t my-4"></div>
    <div class="flex items-center gap-3">
      <div class="min-w-0 flex-1">
        <span class="label">Atajos de teclado</span>
        <p class="text-xs muted">Ver la lista completa y reasignar los que se pueden.</p>
      </div>
      <button class="btn" onclick={onshortcuts}>Abrir…</button>
    </div>
  {/if}

  <div class="divider-t my-4"></div>

  <label class="check">
    <input
      type="checkbox"
      checked={theme.contrast}
      onchange={(event) => theme.setContrast(event.currentTarget.checked)}
    />
    <span>Alto contraste</span>
  </label>
  <p class="mt-1 text-xs muted">
    Oscurece textos secundarios y bordes (o los aclara en el tema oscuro). Se combina con el tema
    que elijas.
  </p>
</Modal>
