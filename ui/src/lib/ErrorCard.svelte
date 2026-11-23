<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { MessageError } from "./query.svelte";
  import { t } from "./i18n.svelte";

  /**
   * Un error de PostgreSQL ordenado: qué falló, el código, dónde, la pista del servidor y lo que se
   * puede hacer enseguida. En una línea de texto rojo se mezclaba todo, y la posición exacta —que
   * ya se marca en el editor— no tenía de dónde agarrarse desde la lista de mensajes.
   */
  let {
    error,
    ongoto,
  }: {
    error: MessageError;
    /** Lleva el cursor al lugar del error; sin posición no hay a dónde ir y el botón no aparece. */
    ongoto?: (at: number) => void;
  } = $props();

  let copied = $state(false);

  /** El texto completo, tal como se pega en un ticket o se busca por su código. */
  function asText(): string {
    return [
      `ERROR ${error.code}: ${error.message}`,
      error.line !== null ? t("Línea {line}", { line: error.line }) : null,
      error.detail ? t("Detalle: {text}", { text: error.detail }) : null,
      error.hint ? t("Pista: {text}", { text: error.hint }) : null,
    ]
      .filter(Boolean)
      .join("\n");
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(asText());
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // El portapapeles lo puede negar el sistema; el texto sigue seleccionable en la tarjeta.
    }
  }
</script>

<div
  class="flex w-full flex-col gap-1.5 rounded-md border border-l-4 border-rose-300 border-l-rose-500 bg-rose-50
         px-3 py-2 font-sans text-xs select-text dark:border-rose-500/30 dark:border-l-rose-500
         dark:bg-rose-500/10"
  role="alert"
>
  <div class="flex flex-wrap items-center gap-2">
    <Icon name="warn" size={13} class="shrink-0 text-rose-600 dark:text-rose-400" />
    <span class="min-w-0 flex-1 text-[13px] font-semibold text-rose-800 dark:text-rose-200">
      {error.message}
    </span>
    <span class="tag tag-bad" title={t("Código SQLSTATE del error")}>{error.code}</span>
    {#if error.line !== null}
      <span class="tag tag-neutral">{t("Línea {line}", { line: error.line })}</span>
    {/if}
  </div>

  {#if error.lineText}
    <code class="block truncate rounded bg-white/70 px-2 py-1 font-mono dark:bg-zinc-900/60">
      {error.lineText}
    </code>
  {/if}

  {#if error.detail}
    <p class="text-zinc-700 dark:text-zinc-300">{error.detail}</p>
  {/if}
  {#if error.hint}
    <p class="text-zinc-600 dark:text-zinc-400"><strong class="font-semibold">{t("Pista:")}</strong> {error.hint}</p>
  {/if}

  <div class="flex items-center gap-1.5 pt-0.5">
    {#if error.at !== null && ongoto}
      <button class="btn btn-sm" onclick={() => ongoto(error.at!)}>
        <Icon name="compass" size={11} /> {t("Ir a la línea")}
      </button>
    {/if}
    <button class="btn btn-sm" onclick={copy}>
      <Icon name="copy" size={11} />
      {copied ? t("Copiado") : t("Copiar error")}
    </button>
  </div>
</div>
