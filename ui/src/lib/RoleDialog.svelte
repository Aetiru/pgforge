<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import { describeError, roleApply, roleMemberships, rolePreview, type RoleInfo } from "./ipc";
  import { roleChanges, roleForm, validateRole } from "./role-form";

  let {
    profileId,
    database,
    existing,
    onclose,
    onsaved,
  }: {
    profileId: string;
    database: string;
    /** `null` da de alta; si no, edita el rol que llega acá. */
    existing: RoleInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // Copia editable, tomada una sola vez: el diálogo se crea de nuevo cada vez que se abre.
  let form = $state(untrack(() => roleForm(existing)));
  let showPassword = $state(false);

  let originalMemberOf = $state<string[]>([]);
  let loadingMemberships = $state(untrack(() => existing !== null));

  let error = $state<string | null>(null);
  let saving = $state(false);
  let preview = $state<string | null>(null);

  $effect(() => {
    if (!existing) return;
    loadingMemberships = true;
    roleMemberships(profileId, existing.name, database)
      .then((result) => {
        originalMemberOf = result;
        form.memberOf = result.join(", ");
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loadingMemberships = false));
  });

  function changes() {
    return roleChanges(form, existing, originalMemberOf);
  }

  function validate(): string | null {
    return validateRole(form);
  }

  async function showPreview() {
    error = null;
    const problem = validate();
    if (problem) {
      error = problem;
      return;
    }
    try {
      const statements = await rolePreview(changes());
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

    if (!(await confirmMutation(profileId, "Se va a modificar un rol del servidor."))) return;

    saving = true;
    try {
      await roleApply(profileId, list, database);
      onsaved();
    } catch (e) {
      error = describeError(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={existing ? `Editar el rol ${existing.name}` : "Nuevo rol"}
  subtitle="Los roles son del servidor entero, no de una base"
  busy={saving}
  {onclose}
>
  <label class="flex flex-col gap-1">
    <span class="label">Nombre</span>
    <input class="field" data-autofocus bind:value={form.name} />
  </label>

  <div class="mt-3">
    <span class="label">Atributos</span>
    <div class="mt-1 grid grid-cols-2 gap-x-4 gap-y-1.5">
      <label class="check"><input type="checkbox" bind:checked={form.login} /> LOGIN</label>
      <label class="check"><input type="checkbox" bind:checked={form.superuser} /> SUPERUSER</label>
      <label class="check"><input type="checkbox" bind:checked={form.createdb} /> CREATEDB</label>
      <label class="check"><input type="checkbox" bind:checked={form.createrole} /> CREATEROLE</label>
      <label class="check"><input type="checkbox" bind:checked={form.inherit} /> INHERIT</label>
      <label class="check"><input type="checkbox" bind:checked={form.replication} /> REPLICATION</label>
      <label class="check"><input type="checkbox" bind:checked={form.bypassRls} /> BYPASSRLS</label>
    </div>
  </div>

  <div class="mt-3 grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Límite de conexiones</span>
      <input
        class="field"
        type="number"
        min="0"
        step="1"
        bind:value={form.connectionLimit}
        placeholder="sin límite"
      />
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Válido hasta</span>
      <input class="field" type="date" bind:value={form.validUntil} />
    </label>
  </div>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">
      Contraseña {existing ? "(vacía = no cambiarla)" : "(opcional)"}
    </span>
    <div class="relative">
      <input
        class="field pr-9"
        type={showPassword ? "text" : "password"}
        bind:value={form.password}
        autocomplete="off"
      />
      <button
        type="button"
        class="btn-icon absolute top-1/2 right-1 -translate-y-1/2"
        title={showPassword ? "Ocultar la contraseña" : "Mostrar la contraseña"}
        aria-label={showPassword ? "Ocultar la contraseña" : "Mostrar la contraseña"}
        onclick={() => (showPassword = !showPassword)}
      >
        <Icon name={showPassword ? "eye-off" : "eye"} size={14} />
      </button>
    </div>
  </label>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">Miembro de (separados por coma)</span>
    {#if loadingMemberships}
      <p
        class="flex items-center gap-2 rounded-md border border-zinc-200 px-2 py-2 text-xs muted
               dark:border-zinc-800"
      >
        <span class="spinner"></span>
        Leyendo las membresías…
      </p>
    {:else}
      <input class="field" bind:value={form.memberOf} placeholder="lectores, escritores" />
    {/if}
  </label>

  {#if existing}
    <label class="check mt-2">
      <input type="checkbox" bind:checked={form.adminOption} />
      Las membresías nuevas quedan con ADMIN OPTION
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
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      {existing ? "Guardar" : "Crear rol"}
    </button>
  {/snippet}
</Modal>
