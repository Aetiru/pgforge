<script lang="ts">
  import { t } from "./i18n.svelte";
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { openMigrations } from "./migrations.svelte";
  import { suggestDescription } from "./migration-view";
  import { describeError, migrationSave, pickOpen, type MigrationSaved } from "./ipc";

  /**
   * Guarda un script como archivo de migración numerado (`V012__descripcion.sql`).
   *
   * Escribe archivos y nada más: no toca el servidor. La migración queda pendiente hasta que se
   * aplica desde la pestaña de migraciones, con su vista previa y su confirmación.
   */
  let {
    sql,
    profileId,
    database,
    schema,
    onclose,
  }: {
    /** El script que se guarda (el del informe, ya filtrado por riesgo). */
    sql: string;
    /** El servidor y la base contra los que se va a aplicar: el destino de la comparación. */
    profileId: string;
    database: string;
    schema: string;
    onclose: () => void;
  } = $props();

  let description = $state(untrack(() => suggestDescription(schema)));
  let folder = $state<string | null>(null);
  let down = $state("");
  let withDown = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let saved = $state<MigrationSaved | null>(null);

  async function choose() {
    const chosen = await pickOpen({ directory: true, title: t("Carpeta de migraciones") });
    if (chosen) folder = chosen;
  }

  function validate(): string | null {
    if (!description.trim()) return t("Poné una descripción: es lo que va en el nombre del archivo.");
    if (!folder) return t("Elegí la carpeta donde guardar la migración.");
    if (withDown && !down.trim()) return t("Escribí el script de reversa, o desmarcá la opción.");
    return null;
  }

  async function submit() {
    error = validate();
    if (error || !folder) return;
    busy = true;
    try {
      saved = await migrationSave(folder, description.trim(), sql, withDown ? down : null);
    } catch (failure) {
      error = describeError(failure);
    } finally {
      busy = false;
    }
  }

  async function openList() {
    if (!folder) return;
    await openMigrations(profileId, database, folder);
    onclose();
  }
</script>

<Modal
  title={t("Guardar como migración")}
  subtitle={t("{schema} en {database}", { schema, database })}
  size="lg"
  {busy}
  {onclose}
>
  {#if saved}
    <Alert tone="ok" box>
      {t("Guardada como")} <span class="font-mono select-text">{saved.upFile}</span>
      {#if saved.downFile}
        {t("y")} <span class="font-mono select-text">{saved.downFile}</span>
      {/if}. {t("Queda pendiente: no se aplicó a ningún servidor.")}
    </Alert>
  {:else}
    <label class="flex flex-col gap-1">
      <span class="label">{t("Descripción")}</span>
      <input class="field" data-autofocus bind:value={description} />
    </label>
    <p class="mt-1 text-xs muted">
      {t("Va en el nombre del archivo, sin tildes ni símbolos:")} <span class="font-mono">V012__sincronizar_public.sql</span>.
      {t("El número es el siguiente a los que ya hay en la carpeta.")}
    </p>

    <div class="mt-3 flex flex-col gap-1">
      <span class="label">{t("Carpeta")}</span>
      <div class="flex gap-2">
        <input class="field flex-1 font-mono" readonly value={folder ?? ""} placeholder={t("Sin elegir")} />
        <button class="btn" onclick={choose}><Icon name="folder" size={12} /> {t("Elegir…")}</button>
      </div>
    </div>

    <label class="check mt-3">
      <input type="checkbox" bind:checked={withDown} />
      {t("Agregar un script de reversa")} (<span class="font-mono">U…</span>)
    </label>
    {#if withDown}
      <textarea
        class="field mt-1 h-28 w-full font-mono text-xs"
        bind:value={down}
        placeholder={t("-- Lo que deja el esquema como estaba antes")}
      ></textarea>
      <p class="mt-1 text-xs muted">
        {t("pgforge no lo deduce: un script de reversa mal armado es peor que ninguno. Sin él, la migración se puede aplicar pero no deshacer desde la aplicación.")}
      </p>
    {/if}

    {#if error}
      <Alert tone="bad" box class="mt-3">{error}</Alert>
    {/if}
  {/if}

  {#snippet footer()}
    {#if saved}
      <button class="btn ml-auto" onclick={onclose}>{t("Cerrar")}</button>
      <button class="btn btn-primary" onclick={openList}>{t("Abrir migraciones")}</button>
    {:else}
      <button class="btn ml-auto" disabled={busy} onclick={onclose}>{t("Cancelar")}</button>
      <button class="btn btn-primary" disabled={busy} onclick={submit}>
        {#if busy}<span class="spinner"></span>{:else}<Icon name="save" size={12} />{/if}
        {t("Guardar")}
      </button>
    {/if}
  {/snippet}
</Modal>
