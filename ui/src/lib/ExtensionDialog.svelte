<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    availableExtensions,
    describeError,
    extensionApply,
    extensionPreview,
    type AvailableExtension,
    type ExtensionChange,
    type ExtensionInfo,
  } from "./ipc";

  let {
    profileId,
    database,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    /** `null` instala una extensión nueva; si no, edita la que llega acá. */
    existing: ExtensionInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // --- Instalar ---
  let available = $state<AvailableExtension[]>([]);
  let loadingAvailable = $state(untrack(() => existing === null));
  let name = $state("");
  let installSchema = $state("");
  let cascade = $state(false);

  // --- Editar ---
  // Versión a la que actualizar; "" es "no actualizar". El esquema arranca en el actual y solo se
  // usa si la extensión es relocatable.
  let updateTo = $state("");
  let schema = $state(untrack(() => existing?.schema ?? ""));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  // Al instalar, se ofrecen solo las extensiones que el paquete tiene pero que todavía no están
  // instaladas: reinstalar una ya presente no tiene sentido.
  const notInstalled = $derived(available.filter((ext) => !ext.installed));
  const selected = $derived(notInstalled.find((ext) => ext.name === name) ?? null);
  const updateAvailable = $derived(
    existing !== null &&
      existing.defaultVersion !== null &&
      existing.defaultVersion !== existing.version,
  );

  $effect(() => {
    if (existing) return;
    loadingAvailable = true;
    availableExtensions(profileId, database)
      .then((list) => {
        available = list;
        const first = list.find((ext) => !ext.installed);
        if (first && !name) name = first.name;
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loadingAvailable = false));
  });

  function changes(): ExtensionChange[] {
    if (!existing) {
      return [
        {
          kind: "create",
          name: name.trim(),
          schema: installSchema.trim() || null,
          version: null,
          cascade,
        },
      ];
    }

    const out: ExtensionChange[] = [];
    if (updateTo && updateTo !== existing.version) {
      out.push({ kind: "update", name: existing.name, version: updateTo });
    }
    const target = schema.trim();
    if (target && target !== existing.schema) {
      out.push({ kind: "setSchema", name: existing.name, schema: target });
    }
    return out;
  }

  function validate(): string | null {
    if (!existing && !name.trim()) return "Elegí una extensión para instalar.";
    return null;
  }

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await extensionPreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n") || "Nada que aplicar.";
    } catch (e) {
      error = describeError(e);
    }
  }

  async function submit() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }

    const list = changes();
    if (list.length === 0) {
      onsaved();
      return;
    }

    saving = true;
    try {
      await extensionApply(profileId, list, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Extensión ${existing.name}` : "Instalar extensión"}
  subtitle="Las extensiones son de la base, no del clúster"
  busy={saving}
  {onclose}
>
  {#if existing}
    <div class="grid grid-cols-2 gap-3">
      <div class="flex flex-col gap-1">
        <span class="label">Versión instalada</span>
        <span class="text-sm select-text">v{existing.version}</span>
      </div>
      <div class="flex flex-col gap-1">
        <span class="label">Esquema</span>
        <span class="text-sm select-text">{existing.schema}</span>
      </div>
    </div>

    {#if existing.comment}
      <p class="mt-2 text-xs muted select-text">{existing.comment}</p>
    {/if}

    {#if updateAvailable}
      <p
        class="mt-3 rounded-md border border-amber-300 bg-amber-50 px-2.5 py-1.5 text-xs
               text-amber-700 dark:border-amber-500/40 dark:bg-amber-500/10 dark:text-amber-300"
      >
        Hay una versión más nueva disponible: v{existing.defaultVersion}.
      </p>
    {/if}

    <label class="mt-3 flex flex-col gap-1">
      <span class="label">Actualizar a la versión</span>
      <select class="field" bind:value={updateTo}>
        <option value="">no actualizar (v{existing.version})</option>
        {#each existing.availableVersions.filter((version) => version !== existing.version) as version (version)}
          <option value={version}>v{version}</option>
        {/each}
      </select>
    </label>

    {#if existing.relocatable}
      <label class="mt-3 flex flex-col gap-1">
        <span class="label">Cambiar de esquema</span>
        <input class="field" bind:value={schema} placeholder={existing.schema} />
      </label>
    {:else}
      <p class="mt-3 text-xs muted">Esta extensión no se puede mover de esquema.</p>
    {/if}
  {:else if loadingAvailable}
    <p class="flex items-center gap-2 text-sm muted">
      <span class="spinner"></span>
      Leyendo las extensiones disponibles…
    </p>
  {:else if notInstalled.length === 0}
    <p class="text-sm muted">Ya están instaladas todas las extensiones que ofrece el servidor.</p>
  {:else}
    <label class="flex flex-col gap-1">
      <span class="label">Extensión</span>
      <select class="field" data-autofocus bind:value={name}>
        {#each notInstalled as ext (ext.name)}
          <option value={ext.name}>{ext.name} (v{ext.defaultVersion ?? "?"})</option>
        {/each}
      </select>
    </label>

    {#if selected?.comment}
      <p class="mt-2 text-xs muted select-text">{selected.comment}</p>
    {/if}

    <label class="mt-3 flex flex-col gap-1">
      <span class="label">Esquema (opcional)</span>
      <input class="field" bind:value={installSchema} placeholder="por omisión de la extensión" />
    </label>

    <label class="check mt-3">
      <input type="checkbox" bind:checked={cascade} />
      CASCADE (instala también las extensiones de las que depende)
    </label>
  {/if}

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button
      class="btn btn-primary"
      onclick={submit}
      disabled={saving || (!existing && notInstalled.length === 0)}
    >
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Instalar"}
    </button>
  {/snippet}
</Modal>
