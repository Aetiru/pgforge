<script lang="ts">
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";
  import { font, FONT_LABELS, type FontChoice } from "./font.svelte";
  import SqlColors from "./SqlColors.svelte";
  import { accent, ACCENTS } from "./accent.svelte";
  import { theme, type ThemePreference } from "./theme.svelte";
  import { editorPrefs } from "./editor/editor-prefs.svelte";
  import { i18n, t } from "./i18n.svelte";
  import { LOCALES } from "./i18n";

  let { onclose, onshortcuts }: { onclose: () => void; onshortcuts?: () => void } = $props();

  const OPTIONS: FontChoice[] = ["source-code-pro", "jetbrains-mono"];

  // Función y no constante: el texto se resuelve al dibujar, o quedaría fijo en el idioma de la carga.
  const themes = (): { id: ThemePreference; label: string }[] => [
    { id: "system", label: t("Sistema") },
    { id: "light", label: t("Claro") },
    { id: "dark", label: t("Oscuro") },
    { id: "paper", label: t("Papel") },
  ];
</script>

<Modal title={t("Preferencias")} subtitle={t("Vale para toda la aplicación")} size="lg" {onclose}>
  <div class="flex flex-col gap-1.5">
    <span class="label">{t("Idioma")}</span>
    <div class="seg self-start" role="group" aria-label={t("Idioma")}>
      {#each LOCALES as option (option.value)}
        <button
          class="seg-item"
          aria-pressed={i18n.current === option.value}
          onclick={() => i18n.set(option.value)}
        >
          {option.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="divider-t my-4"></div>

  <div class="flex flex-col gap-1.5">
    <span class="label">{t("Tema")}</span>
    <div class="seg self-start" role="group" aria-label={t("Tema")}>
      {#each themes() as option (option.id)}
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
      {t("«Papel» es un claro de fondos crema y acento verde azulado, más cálido que el claro común.")}
    </p>
  </div>

  <div class="divider-t my-4"></div>

  <div class="flex flex-col gap-1.5">
    <span class="label">{t("Color de acento")}</span>
    <div class="flex items-center gap-2" role="group" aria-label={t("Color de acento")}>
      {#each ACCENTS as option (option.id)}
        <button
          class="size-7 rounded-full border-2 border-white shadow-sm ring-1 ring-zinc-300
                 transition-transform hover:scale-110 dark:border-zinc-900 dark:ring-zinc-600
                 {accent.choice === option.id ? 'ring-2 ring-zinc-900 dark:ring-zinc-100' : ''}"
          style="background-color: {option.swatch}"
          aria-pressed={accent.choice === option.id}
          aria-label={t(option.label)}
          title={t(option.label)}
          onclick={() => accent.set(option.id)}
        ></button>
      {/each}
    </div>
    <p class="text-xs muted">
      {t("Cambia pestañas activas, selección y enlaces. El rojo de producción, el ámbar de la transacción y el verde de ejecutar no cambian nunca.")}
    </p>
  </div>

  <div class="divider-t my-4"></div>

  <div class="flex flex-col gap-1.5">
    <span class="label">{t("Tipo de letra del código y los datos")}</span>
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
        <span class="label">{t("Atajos de teclado")}</span>
        <p class="text-xs muted">{t("Ver la lista completa y reasignar los que se pueden.")}</p>
      </div>
      <button class="btn" onclick={onshortcuts}>{t("Abrir…")}</button>
    </div>
  {/if}

  <div class="divider-t my-4"></div>

  <label class="check">
    <input
      type="checkbox"
      checked={theme.contrast}
      onchange={(event) => theme.setContrast(event.currentTarget.checked)}
    />
    <span>{t("Alto contraste")}</span>
  </label>
  <p class="mt-1 text-xs muted">
    {t("Oscurece textos secundarios y bordes (o los aclara en el tema oscuro). Se combina con el tema que elijas.")}
  </p>

  <div class="divider-t my-4"></div>

  <span class="label">{t("Ayudas del editor de SQL")}</span>
  <div class="mt-1.5 flex flex-col gap-2">
    <div>
      <label class="check">
        <input
          type="checkbox"
          checked={editorPrefs.check}
          onchange={(event) => editorPrefs.set("check", event.currentTarget.checked)}
        />
        <span>{t("Subrayar errores mientras escribo")}</span>
      </label>
      <p class="ml-6 text-xs muted">
        {t("El servidor revisa la sentencia sin ejecutarla. Se salta con una transacción abierta y no toca nada.")}
      </p>
    </div>
    <div>
      <label class="check">
        <input
          type="checkbox"
          checked={editorPrefs.lint}
          onchange={(event) => editorPrefs.set("lint", event.currentTarget.checked)}
        />
        <span>{t("Avisar de consultas riesgosas")}</span>
      </label>
      <p class="ml-6 text-xs muted">
        <span class="font-mono">= NULL</span>, <span class="font-mono">NOT IN</span> {t("con subconsulta,")} <span class="font-mono">SELECT *</span> {t("sin límite…")}
      </p>
    </div>
    <div>
      <label class="check">
        <input
          type="checkbox"
          checked={editorPrefs.ghost}
          onchange={(event) => editorPrefs.set("ghost", event.currentTarget.checked)}
        />
        <span>{t("Sugerir desde mi historial")}</span>
      </label>
      <p class="ml-6 text-xs muted">
        {t("Un texto gris continúa con lo que ya ejecutaste;")} <span class="font-mono">Tab</span> {t("lo acepta.")}
      </p>
    </div>
  </div>
</Modal>
