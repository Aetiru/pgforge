<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    roleApply,
    roleMemberships,
    rolePreview,
    type RoleAttributes,
    type RoleChange,
    type RoleInfo,
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
    /** `null` da de alta; si no, edita el rol que llega acá. */
    existing: RoleInfo | null;
    onclose: () => void;
    onsaved: () => void;
  } = $props();

  // Copia editable, tomada una sola vez: el diálogo se crea de nuevo cada vez que se abre.
  let name = $state(untrack(() => existing?.name ?? ""));
  let superuser = $state(untrack(() => existing?.superuser ?? false));
  let createdb = $state(untrack(() => existing?.createdb ?? false));
  let createrole = $state(untrack(() => existing?.createrole ?? false));
  let inherit = $state(untrack(() => existing?.inherit ?? true));
  let login = $state(untrack(() => existing?.login ?? false));
  let replication = $state(untrack(() => existing?.replication ?? false));
  let bypassRls = $state(untrack(() => existing?.bypassRls ?? false));
  let connectionLimitText = $state(
    untrack(() => (existing && existing.connectionLimit !== -1 ? String(existing.connectionLimit) : "")),
  );
  /** Siempre arranca vacía: Postgres nunca devuelve la contraseña para precargarla. */
  let password = $state("");
  let validUntil = $state(untrack(() => existing?.validUntil ?? ""));
  let memberOfText = $state("");
  let adminOption = $state(false);

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
        memberOfText = result.join(", ");
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loadingMemberships = false));
  });

  function memberOfList(): string[] {
    return memberOfText
      .split(",")
      .map((r) => r.trim())
      .filter((r) => r.length > 0);
  }

  function parsedConnectionLimit(): number {
    const trimmed = connectionLimitText.trim();
    return trimmed === "" ? -1 : Number(trimmed);
  }

  function changes(): RoleChange[] {
    if (!existing) {
      const attributes: RoleAttributes = {
        superuser,
        createdb,
        createrole,
        inherit,
        login,
        replication,
        bypassRls,
        connectionLimit: parsedConnectionLimit(),
        password: password || undefined,
        validUntil: validUntil.trim() || undefined,
      };
      return [{ kind: "createRole", name: name.trim(), attributes, memberOf: memberOfList() }];
    }

    const out: RoleChange[] = [];
    let currentName = existing.name;
    if (name.trim() !== existing.name) {
      out.push({ kind: "renameRole", name: currentName, newName: name.trim() });
      currentName = name.trim();
    }

    const attrs: RoleAttributes = {};
    let anyAttr = false;
    const setIfChanged = <K extends keyof RoleAttributes>(key: K, value: RoleAttributes[K], original: RoleAttributes[K]) => {
      if (value !== original) {
        attrs[key] = value;
        anyAttr = true;
      }
    };
    setIfChanged("superuser", superuser, existing.superuser);
    setIfChanged("createdb", createdb, existing.createdb);
    setIfChanged("createrole", createrole, existing.createrole);
    setIfChanged("inherit", inherit, existing.inherit);
    setIfChanged("login", login, existing.login);
    setIfChanged("replication", replication, existing.replication);
    setIfChanged("bypassRls", bypassRls, existing.bypassRls);
    setIfChanged("connectionLimit", parsedConnectionLimit(), existing.connectionLimit);
    if (password.trim()) {
      attrs.password = password;
      anyAttr = true;
    }
    const until = validUntil.trim();
    if (until !== (existing.validUntil ?? "")) {
      // Vaciar el campo vuelve el rol a "sin vencimiento", que en Postgres es 'infinity'.
      attrs.validUntil = until || "infinity";
      anyAttr = true;
    }
    if (anyAttr) out.push({ kind: "alterRole", name: currentName, attributes: attrs });

    const current = memberOfList();
    for (const role of current.filter((r) => !originalMemberOf.includes(r))) {
      out.push({ kind: "grantMembership", role, member: currentName, adminOption });
    }
    for (const role of originalMemberOf.filter((r) => !current.includes(r))) {
      out.push({ kind: "revokeMembership", role, member: currentName });
    }

    return out;
  }

  function validate(): string | null {
    if (!name.trim()) return "Poné un nombre para el rol.";
    if (connectionLimitText.trim() !== "" && Number.isNaN(Number(connectionLimitText.trim()))) {
      return "El límite de conexiones tiene que ser un número.";
    }
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
    <input class="field" data-autofocus bind:value={name} />
  </label>

  <div class="mt-3">
    <span class="label">Atributos</span>
    <div class="mt-1 grid grid-cols-2 gap-x-4 gap-y-1.5">
      <label class="check"><input type="checkbox" bind:checked={login} /> LOGIN</label>
      <label class="check"><input type="checkbox" bind:checked={superuser} /> SUPERUSER</label>
      <label class="check"><input type="checkbox" bind:checked={createdb} /> CREATEDB</label>
      <label class="check"><input type="checkbox" bind:checked={createrole} /> CREATEROLE</label>
      <label class="check"><input type="checkbox" bind:checked={inherit} /> INHERIT</label>
      <label class="check"><input type="checkbox" bind:checked={replication} /> REPLICATION</label>
      <label class="check"><input type="checkbox" bind:checked={bypassRls} /> BYPASSRLS</label>
    </div>
  </div>

  <div class="mt-3 grid grid-cols-2 gap-3">
    <label class="flex flex-col gap-1">
      <span class="label">Límite de conexiones</span>
      <input class="field" bind:value={connectionLimitText} placeholder="sin límite" />
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Válido hasta</span>
      <input class="field" bind:value={validUntil} placeholder="sin vencimiento" />
    </label>
  </div>

  <label class="mt-3 flex flex-col gap-1">
    <span class="label">
      Contraseña {existing ? "(vacía = no cambiarla)" : "(opcional)"}
    </span>
    <input class="field" type="password" bind:value={password} autocomplete="off" />
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
      <input class="field" bind:value={memberOfText} placeholder="lectores, escritores" />
    {/if}
  </label>

  {#if existing}
    <label class="check mt-2">
      <input type="checkbox" bind:checked={adminOption} />
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
