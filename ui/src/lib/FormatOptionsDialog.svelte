<script lang="ts">
  import Alert from "./Alert.svelte";
  import { t } from "./i18n.svelte";
  import Modal from "./Modal.svelte";
  import Sql from "./Sql.svelte";
  import { FORMAT_SAMPLE, isDefaultFormat } from "./format-options";
  import { formatPrefs } from "./format-options.svelte";
  import { describeError, sqlFormat, type FormatOptions } from "./ipc";

  let { onclose }: { onclose: () => void } = $props();

  // El ejemplo se pide al formateador **real**: si lo dibujara la interfaz, mostraría lo que cree que
  // hace el núcleo y no lo que hace.
  let sample = $state("");
  let error = $state<string | null>(null);
  let run = 0;

  $effect(() => {
    const options = $state.snapshot(formatPrefs.options);
    const mine = ++run;
    const timer = setTimeout(() => {
      sqlFormat(FORMAT_SAMPLE, options)
        .then((text) => {
          if (mine !== run) return;
          sample = text;
          error = null;
        })
        .catch((e) => {
          if (mine === run) error = describeError(e);
        });
    }, 150);
    return () => clearTimeout(timer);
  });

  // Derivadas y no constantes: la etiqueta se traduce al idioma vigente, no al del primer render.
  const CASES = $derived([
    { value: "upper", label: "SELECT" },
    { value: "lower", label: "select" },
    { value: "preserve", label: t("Como está") },
  ] as const);
  const INDENTS = $derived([
    { value: "2", label: t("2 espacios") },
    { value: "4", label: t("4 espacios") },
    { value: "tab", label: t("Tabulador") },
  ] as const);
  const LINES = [0, 1, 2, 3];

  const options = $derived(formatPrefs.options);
  const isDefault = $derived(isDefaultFormat(options));

  const set = (next: Partial<FormatOptions>) => formatPrefs.set(next);
</script>

<Modal title={t("Opciones del formateador")} size="lg" {onclose}>
  <div class="grid gap-4 md:grid-cols-[14rem_minmax(0,1fr)]">
    <div class="flex flex-col gap-3 text-xs">
      <div>
        <div class="label">{t("Palabras clave")}</div>
        <div class="seg" role="group" aria-label={t("Palabras clave")}>
          {#each CASES as item (item.value)}
            <button
              class="seg-item"
              aria-pressed={options.keywordCase === item.value}
              onclick={() => set({ keywordCase: item.value })}>{item.label}</button
            >
          {/each}
        </div>
        <p class="mt-1 muted">{t("Los nombres de tablas y columnas nunca cambian de caso.")}</p>
      </div>

      <div>
        <div class="label">{t("Sangría")}</div>
        <div class="seg" role="group" aria-label={t("Sangría")}>
          {#each INDENTS as item (item.value)}
            <button
              class="seg-item"
              aria-pressed={options.indent === item.value}
              onclick={() => set({ indent: item.value })}>{item.label}</button
            >
          {/each}
        </div>
      </div>

      <div>
        <label class="label" for="fmt-lines">{t("Líneas en blanco entre sentencias")}</label>
        <select
          id="fmt-lines"
          class="field w-full"
          value={options.linesBetweenQueries}
          onchange={(event) => set({ linesBetweenQueries: Number(event.currentTarget.value) })}
        >
          {#each LINES as n (n)}
            <option value={n}>{n}</option>
          {/each}
        </select>
      </div>

      <label class="check">
        <input
          type="checkbox"
          checked={options.commaFirst}
          onchange={(event) => set({ commaFirst: event.currentTarget.checked })}
        />
        {t("Coma al principio de la línea")}
      </label>
    </div>

    <div class="min-w-0">
      <div class="label">{t("Así queda")}</div>
      <div class="max-h-80 overflow-auto rounded-md border border-zinc-200 p-2 select-text dark:border-zinc-700">
        {#if sample}<Sql code={sample} />{/if}
      </div>
      {#if error}<Alert tone="bad" box class="mt-2">{error}</Alert>{/if}
      <p class="mt-2 text-xs muted">
        {t("Vale para el botón y el atajo de formatear, el comando de la paleta y el autoformateo. Solo cambia espacios y la caja de las palabras clave: el formateador comprueba que el SQL resultante sea el mismo.")}
      </p>
    </div>
  </div>

  {#snippet footer()}
    <button class="btn" disabled={isDefault} onclick={() => formatPrefs.reset()}>
      {t("Valores de fábrica")}
    </button>
    <button class="btn btn-primary ml-auto" onclick={onclose}>{t("Listo")}</button>
  {/snippet}
</Modal>
