<script lang="ts">
  import { t } from "./i18n.svelte";
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { MAX_NOTE_LENGTH, normalizeTags } from "./notes";
  import { notes } from "./notes.svelte";

  /**
   * La nota y las etiquetas de un objeto, a la vista arriba del detalle. Plegado muestra una línea
   * (o un «Agregar nota» discreto); desplegado, el cuadro de texto y las etiquetas. Se guarda al
   * salir de cada campo.
   */
  let {
    profileId,
    database,
    schema,
    name,
  }: { profileId: string; database: string; schema: string; name: string } = $props();

  const saved = $derived(notes.get(profileId, database, schema, name));
  let open = $state(false);
  let text = $state("");
  let tagsText = $state("");

  // Al cambiar de objeto se vuelve a cargar y se pliega.
  $effect(() => {
    const current = notes.get(profileId, database, schema, name);
    untrack(() => {
      text = current?.note ?? "";
      tagsText = (current?.tags ?? []).map((tag) => `#${tag}`).join(" ");
      open = false;
    });
  });

  function save() {
    notes.set(profileId, database, schema, name, {
      note: text.slice(0, MAX_NOTE_LENGTH),
      tags: normalizeTags(tagsText),
    });
  }
</script>

<div class="divider-b px-2 py-1.5 text-xs @md/detail:px-4">
  {#if !open}
    <button
      class="flex w-full items-center gap-2 text-left muted hover:text-zinc-900 dark:hover:text-zinc-100"
      onclick={() => (open = true)}
      title={t("Notas y etiquetas de este objeto, guardadas en esta computadora")}
    >
      <Icon name="edit" size={12} />
      {#if saved}
        {#each saved.tags as tag (tag)}<span class="tag tag-info">#{tag}</span>{/each}
        <span class="min-w-0 flex-1 truncate">{saved.note}</span>
      {:else}
        <span>{t("Agregar nota o etiquetas")}</span>
      {/if}
    </button>
  {:else}
    <div class="flex flex-col gap-1.5">
      <textarea
        class="field min-h-16 resize-y text-xs"
        placeholder={t("Qué es, quién la usa, qué no tocar…")}
        maxlength={MAX_NOTE_LENGTH}
        bind:value={text}
        onblur={save}
      ></textarea>
      <input
        class="field text-xs"
        placeholder={t("#etiqueta #otra")}
        bind:value={tagsText}
        onblur={save}
        spellcheck="false"
      />
      <div class="flex items-center gap-2 muted">
        <span>{t("Se guarda en esta computadora, no en la base.")}</span>
        <button
          class="btn btn-sm ml-auto"
          onclick={() => {
            save();
            open = false;
          }}>{t("Listo")}</button
        >
      </div>
    </div>
  {/if}
</div>
