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
    describeError,
    userMappingApply,
    userMappingPreview,
    type UserMapping,
    type UserMappingChange,
  } from "./ipc";

  let {
    profileId,
    database,
    server,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    server: string;
    /** `null` da de alta; si no, edita el mapeo de ese rol. */
    existing: UserMapping | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  let user = $state(untrack(() => existing?.user ?? ""));
  let rows = $state<OptionRow[]>(untrack(() => rowsFrom(existing?.options ?? [])));
  // Las opciones vienen ocultas cuando el rol conectado no puede verlas: no se puede editar a ciegas.
  const optionsHidden = untrack(() => existing !== null && existing.options === null);

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  function changes(): UserMappingChange[] {
    if (!existing) {
      return [{ kind: "create", server, user: user.trim(), options: toOptions(rows) }];
    }
    const options = toDelta(existing.options ?? [], rows);
    if (deltaIsEmpty(options)) return [];
    return [{ kind: "alter", server, user: existing.user, options }];
  }

  function validate(): string | null {
    if (!existing && !user.trim()) return "Poné el rol del mapeo (o PUBLIC).";
    return null;
  }

  async function showPreview() {
    error = validate();
    if (error) return;
    try {
      const statements = await userMappingPreview(changes());
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
      await userMappingApply(profileId, list, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Mapeo de ${existing.user}` : "Nuevo mapeo de usuario"}
  subtitle="USER MAPPING · servidor {server}"
  busy={saving}
  {onclose}
>
  <label class="flex flex-col gap-1">
    <span class="label">Rol</span>
    <input
      class="field"
      data-autofocus
      bind:value={user}
      disabled={existing !== null}
      placeholder="rol, o PUBLIC / CURRENT_USER"
    />
  </label>

  <div class="mt-3">
    <span class="label">Opciones</span>
    <p class="mb-1 text-xs muted">Para postgres_fdw: user, password.</p>
    {#if optionsHidden}
      <Alert tone="warn" box>
        No se pueden ver las opciones de este mapeo (hay que ser su dueño o superusuario). Lo que
        agregues acá se suma; lo existente no se toca.
      </Alert>
    {/if}
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
