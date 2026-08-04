<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import OptionsEditor, {
    deltaIsEmpty,
    rowsFrom,
    toDelta,
    toOptions,
    type OptionRow,
  } from "./OptionsEditor.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    availableFdws,
    describeError,
    foreignServerApply,
    foreignServerPreview,
    type ServerChange,
    type ServerInfo,
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
    existing: ServerInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let name = $state(untrack(() => existing?.name ?? ""));
  let fdw = $state(untrack(() => existing?.fdw ?? ""));
  let serverType = $state(untrack(() => existing?.serverType ?? ""));
  let version = $state(untrack(() => existing?.version ?? ""));
  let rows = $state<OptionRow[]>(untrack(() => rowsFrom(existing?.options ?? [])));

  let fdws = $state<string[]>([]);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  // Al crear hace falta elegir el wrapper: se traen los disponibles y se preselecciona el primero.
  $effect(() => {
    if (existing) return;
    availableFdws(profileId, database)
      .then((list) => {
        fdws = list;
        if (!fdw && list.length > 0) fdw = list[0];
      })
      .catch((e) => (error = describeError(e)));
  });

  function changes(): ServerChange[] {
    if (!existing) {
      return [
        {
          kind: "create",
          name: name.trim(),
          fdw,
          serverType: serverType.trim() || null,
          version: version.trim() || null,
          options: toOptions(rows),
        },
      ];
    }

    // El TYPE no se puede alterar; solo la versión y las opciones.
    const nextVersion = version.trim();
    const prevVersion = existing.version ?? "";
    const options = toDelta(existing.options, rows);
    const versionChanged = nextVersion !== prevVersion;
    if (!versionChanged && deltaIsEmpty(options)) return [];

    return [
      {
        kind: "alter",
        name: existing.name,
        version: versionChanged ? nextVersion || null : null,
        options,
      },
    ];
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para el servidor.";
    if (!existing && !fdw) return "Elegí el wrapper del servidor.";
    return null;
  }

  async function showPreview() {
    error = validate();
    if (error) return;
    try {
      const statements = await foreignServerPreview(changes());
      preview = statements.map((statement) => statement.sql).join(";\n\n") || "Nada que aplicar.";
    } catch (e) {
      error = describeError(e);
    }
  }

  async function submit() {
    error = validate();
    if (error) return;
    const list = changes();
    if (list.length === 0) {
      onsaved();
      return;
    }
    saving = true;
    try {
      await foreignServerApply(profileId, list, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Servidor ${existing.name}` : "Nuevo servidor foráneo"}
  subtitle="SERVER"
  busy={saving}
  {onclose}
>
  <label class="flex flex-col gap-1">
    <span class="label">Nombre</span>
    <input class="field" data-autofocus bind:value={name} disabled={existing !== null} />
  </label>

  <div class="mt-3 grid grid-cols-3 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Wrapper</span>
      {#if existing}
        <input class="field" value={existing.fdw} disabled />
      {:else}
        <select class="field" bind:value={fdw}>
          {#each fdws as option (option)}
            <option value={option}>{option}</option>
          {/each}
        </select>
      {/if}
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Tipo (opcional)</span>
      <input class="field" bind:value={serverType} disabled={existing !== null} />
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Versión (opcional)</span>
      <input class="field" bind:value={version} />
    </label>
  </div>

  <div class="mt-3">
    <span class="label">Opciones</span>
    <p class="mb-1 text-xs muted">Para postgres_fdw: host, port, dbname, …</p>
    <OptionsEditor bind:rows />
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  {#if preview}
    <SqlPreview sql={preview} />
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost btn-sm" onclick={showPreview} disabled={saving}>Ver SQL</button>
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>Cancelar</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Crear"}
    </button>
  {/snippet}
</Modal>
