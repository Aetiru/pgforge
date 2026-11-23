<script lang="ts">
  import { untrack } from "svelte";
  import { confirmMutation } from "./access.svelte";
  import Alert from "./Alert.svelte";
  import Modal from "./Modal.svelte";
  import { t } from "./i18n.svelte";
  import SqlPreview from "./SqlPreview.svelte";
  import {
    describeError,
    roleApply,
    roleMemberships,
    roleOptions,
    rolePreview,
    schemaNames,
    type RoleInfo,
    type RoleOption,
  } from "./ipc";
  import {
    addDays,
    dangerousAttributes,
    expiryPresetOf,
    generatePassword,
    roleAttributes,
    roleChanges,
    roleForm,
    roleSummary,
    validateRole,
    type ExpiryPreset,
    type RoleKind,
  } from "./role-form";

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

  let originalMemberOf = $state<string[]>([]);
  let loadingMemberships = $state(untrack(() => existing !== null));
  /** Los roles del servidor entre los que se elige; los grupos vienen primero. Vacío mientras se leen. */
  let allRoles = $state<RoleOption[]>([]);
  /** Los esquemas de la base conectada: la plantilla de acceso es de esa base y no de otra. */
  let schemas = $state<string[]>([]);

  let error = $state<string | null>(null);
  let saving = $state(false);

  /** Vencimiento «a mano»: sin esto una fecha elegida y «nunca» no se distinguen cuando el campo está vacío. */
  let customExpiry = $state(untrack(() => expiryPresetOf(form.validUntil) === "custom"));
  let copied = $state(false);
  let passwordInput = $state<HTMLInputElement | null>(null);

  // Alta: lo que se muestra lo decide el tipo elegido. Edición: el tipo no se cambia, pero un grupo
  // al que se le prende LOGIN (desde «Avanzado») pasa a pedir contraseña.
  const isUser = $derived(existing ? form.login : form.kind === "user");

  $effect(() => {
    if (!existing) return;
    loadingMemberships = true;
    roleMemberships(profileId, existing.name, database)
      .then((result) => {
        originalMemberOf = result;
        form.memberOf = [...result];
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loadingMemberships = false));
  });

  // La lista de roles se lee siempre, también al dar de alta: elegir de una lista es justamente lo
  // que evita el `GRANT` que falla por un nombre mal escrito.
  $effect(() => {
    roleOptions(profileId, database)
      .then((result) => (allRoles = result))
      .catch((e) => (error = describeError(e)));
  });

  $effect(() => {
    if (existing) return;
    schemaNames(profileId, database)
      .then((result) => {
        schemas = result;
        if (!form.access.schema) form.access.schema = result.includes("public") ? "public" : (result[0] ?? "");
      })
      .catch((e) => (error = describeError(e)));
  });

  /**
   * Qué roles se ofrecen para agregar: los que no están ya elegidos ni es el propio. Las membresías
   * que ya tiene van como fichas aunque `role_options` no las devuelva —un rol predefinido de
   * Postgres queda afuera—: si no aparecieran, guardar sin tocar nada las revocaría en silencio.
   */
  const addable = $derived.by(() => {
    const self = form.name.trim();
    return allRoles.filter((role) => role.name !== self && !form.memberOf.includes(role.name));
  });
  const addableGroups = $derived(addable.filter((role) => !role.login));
  const addableUsers = $derived(addable.filter((role) => role.login));

  function addMember(event: Event & { currentTarget: HTMLSelectElement }) {
    const name = event.currentTarget.value;
    event.currentTarget.value = "";
    if (name && !form.memberOf.includes(name)) form.memberOf = [...form.memberOf, name];
  }

  function removeMember(name: string) {
    form.memberOf = form.memberOf.filter((role) => role !== name);
  }

  function setKind(kind: RoleKind) {
    form.kind = kind;
    // `login` se mantiene coherente con el tipo: es lo que lee el resto del formulario.
    form.login = kind === "user";
  }

  const preset = $derived<ExpiryPreset>(customExpiry ? "custom" : expiryPresetOf(form.validUntil));

  function setExpiry(next: ExpiryPreset) {
    customExpiry = next === "custom";
    if (next === "never") form.validUntil = "";
    else if (next !== "custom") form.validUntil = addDays(Number(next));
  }

  const expiryChoices = $derived<{ id: ExpiryPreset; label: string }[]>([
    { id: "never", label: t("Nunca") },
    { id: "30", label: t("30 días") },
    { id: "90", label: t("90 días") },
    { id: "365", label: t("1 año") },
    { id: "custom", label: t("Elegir fecha") },
  ]);

  const accessChoices = $derived<{ id: "none" | "read" | "write" | "admin"; label: string }[]>([
    { id: "none", label: t("Sin acceso") },
    { id: "read", label: t("Solo lectura") },
    { id: "write", label: t("Lectura y escritura") },
    { id: "admin", label: t("Administra el esquema") },
  ]);

  function generate() {
    form.password = generatePassword();
    copied = false;
  }

  async function copyPassword() {
    if (!form.password) return;
    try {
      await navigator.clipboard.writeText(form.password);
      copied = true;
      setTimeout(() => (copied = false), 1800);
    } catch {
      // Sin permiso de portapapeles (o sin contexto seguro): se deja el texto seleccionado para Ctrl+C.
      passwordInput?.focus();
      passwordInput?.select();
    }
  }

  const attributes = $derived(roleAttributes());
  const risky = $derived(dangerousAttributes(form));
  const summary = $derived(roleSummary(form));

  function changes() {
    return roleChanges(form, existing, originalMemberOf);
  }

  function validate(): string | null {
    return validateRole(form);
  }

  // El SQL en vivo. Se pide al mismo comando que la vista previa de siempre, con una pausa para no
  // preguntar en cada tecla y descartando la respuesta de un texto que ya cambió.
  let liveSql = $state<string | null>(null);
  let liveFailure = $state<string | null>(null);
  let liveSeq = 0;

  $effect(() => {
    const list = $state.snapshot(changes());
    const ready = validate() === null && !loadingMemberships;
    const seq = ++liveSeq;
    if (!ready) {
      liveSql = null;
      liveFailure = null;
      return;
    }
    const timer = setTimeout(() => {
      rolePreview(list)
        .then((statements) => {
          if (seq !== liveSeq) return;
          liveSql = statements.map((statement) => statement.sql).join(";\n\n");
          liveFailure = null;
        })
        .catch((e) => {
          if (seq !== liveSeq) return;
          liveSql = null;
          liveFailure = describeError(e);
        });
    }, 250);
    return () => clearTimeout(timer);
  });

  /**
   * El SQL que se va a ejecutar, para que la confirmación de producción sepa si trae algo
   * destructivo. Reutiliza el que ya está a la vista si es el vigente.
   */
  async function sqlToApply(): Promise<string> {
    const statements = await rolePreview(changes());
    return statements.map((statement) => statement.sql).join(";\n\n");
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

    if (!(await confirmMutation(profileId, t("Se va a modificar un rol del servidor."), sqlToApply))) return;

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

  const submitLabel = $derived(
    existing ? t("Guardar") : form.kind === "group" ? t("Crear grupo") : t("Crear usuario"),
  );
</script>

<Modal
  title={existing ? t("Editar el rol {name}", { name: existing.name }) : t("Nuevo rol")}
  subtitle={t("Los roles son del servidor entero, no de una base")}
  size="2xl"
  busy={saving}
  {onclose}
>
  <div class="grid gap-5 lg:grid-cols-[minmax(0,1fr)_20rem]">
    <div class="flex min-w-0 flex-col gap-4">
      {#if existing}
        <div class="flex items-center gap-2">
          <span class="label">{t("Tipo")}</span>
          <span class="tag-neutral">{existing.login ? t("Usuario") : t("Grupo")}</span>
        </div>
      {:else}
        <div class="flex flex-col gap-1">
          <span class="label" id="role-kind-label">{t("¿Qué vas a crear?")}</span>
          <div class="grid grid-cols-2 gap-2" role="group" aria-labelledby="role-kind-label">
            <button
              type="button"
              class="card p-2.5 text-left transition-colors hover:border-zinc-400 aria-pressed:border-blue-500
                     aria-pressed:ring-2 aria-pressed:ring-blue-500/30 dark:hover:border-zinc-500"
              aria-pressed={form.kind === "user"}
              onclick={() => setKind("user")}
            >
              <span class="block text-sm font-medium">{t("Usuario")}</span>
              <span class="block text-xs muted">{t("Una persona o una aplicación que inicia sesión.")}</span>
            </button>
            <button
              type="button"
              class="card p-2.5 text-left transition-colors hover:border-zinc-400 aria-pressed:border-blue-500
                     aria-pressed:ring-2 aria-pressed:ring-blue-500/30 dark:hover:border-zinc-500"
              aria-pressed={form.kind === "group"}
              onclick={() => setKind("group")}
            >
              <span class="block text-sm font-medium">{t("Grupo")}</span>
              <span class="block text-xs muted">
                {t("Reúne permisos. Otros roles entran en él. No inicia sesión.")}
              </span>
            </button>
          </div>
        </div>
      {/if}

      <label class="flex flex-col gap-1">
        <span class="label">{t("Nombre")}</span>
        <input class="field" data-autofocus bind:value={form.name} autocomplete="off" spellcheck="false" />
      </label>

      {#if isUser}
        <div class="flex flex-col gap-1">
          <label class="label" for="role-password">
            {existing ? t("Contraseña (vacía = no cambiarla)") : t("Contraseña (opcional)")}
          </label>
          <div class="flex flex-wrap items-center gap-2">
            <input
              id="role-password"
              class="field min-w-40 flex-1 font-mono"
              type="text"
              bind:this={passwordInput}
              bind:value={form.password}
              oninput={() => (copied = false)}
              autocomplete="off"
              spellcheck="false"
            />
            <button type="button" class="btn" onclick={generate}>{t("Generar")}</button>
            <button type="button" class="btn" onclick={copyPassword} disabled={!form.password}>
              {copied ? t("Copiada") : t("Copiar")}
            </button>
          </div>
          <span class="text-xs muted">
            {t("Se muestra una sola vez. Después solo se puede cambiar, no leer.")}
          </span>
        </div>

        <div class="flex flex-col gap-1">
          <span class="label" id="role-expiry-label">{t("Vence")}</span>
          <div class="flex flex-wrap items-center gap-2">
            <div class="seg" role="group" aria-labelledby="role-expiry-label">
              {#each expiryChoices as choice (choice.id)}
                <button
                  type="button"
                  class="seg-item"
                  aria-pressed={preset === choice.id}
                  onclick={() => setExpiry(choice.id)}
                >
                  {choice.label}
                </button>
              {/each}
            </div>
            {#if customExpiry}
              <input
                class="field"
                type="date"
                bind:value={form.validUntil}
                aria-label={t("Fecha de vencimiento")}
              />
            {:else if form.validUntil}
              <span class="text-xs muted">{t("Vence el {date}.", { date: form.validUntil })}</span>
            {/if}
          </div>
        </div>
      {/if}

      <div class="flex flex-col gap-1">
        <span class="label" id="role-groups-label">
          {existing ? t("Miembro de") : t("Pertenece a estos grupos")}
        </span>
        {#if loadingMemberships}
          <p class="flex items-center gap-2 text-xs muted">
            <span class="spinner"></span>
            {t("Leyendo las membresías…")}
          </p>
        {:else}
          <ul class="flex flex-wrap gap-1.5" aria-labelledby="role-groups-label">
            {#each form.memberOf as name (name)}
              <li class="tag-neutral flex items-center gap-1">
                <span class="font-mono">{name}</span>
                <button
                  type="button"
                  class="rounded px-0.5 leading-none hover:text-rose-600"
                  title={t("Quitar {name}", { name })}
                  aria-label={t("Quitar {name}", { name })}
                  onclick={() => removeMember(name)}
                >
                  ×
                </button>
              </li>
            {:else}
              <li class="text-xs muted">{t("No pertenece a ningún grupo.")}</li>
            {/each}
          </ul>
          <select
            class="field max-w-72"
            aria-label={t("Agregar un grupo…")}
            value=""
            onchange={addMember}
            disabled={addable.length === 0}
          >
            <option value="">{t("Agregar un grupo…")}</option>
            {#if addableGroups.length > 0}
              <optgroup label={t("Grupos (no inician sesión)")}>
                {#each addableGroups as role (role.name)}
                  <option value={role.name}>{role.name}</option>
                {/each}
              </optgroup>
            {/if}
            {#if addableUsers.length > 0}
              <optgroup label={t("Otros roles")}>
                {#each addableUsers as role (role.name)}
                  <option value={role.name}>{role.name}</option>
                {/each}
              </optgroup>
            {/if}
          </select>
          <span class="text-xs muted">
            {t("Los permisos del grupo los hereda quien entra en él.")}
          </span>
        {/if}
      </div>

      {#if !existing}
        <div class="flex flex-col gap-2">
          <span class="label" id="role-access-label">{t("Plantilla de acceso")}</span>
          <div class="flex flex-wrap items-center gap-2">
            <select
              class="field max-w-48"
              aria-label={t("Esquema de la base {database}", { database: database || "—" })}
              title={t("Esquema de la base {database}", { database: database || "—" })}
              bind:value={form.access.schema}
            >
              {#each schemas as schema (schema)}
                <option value={schema}>{schema}</option>
              {/each}
            </select>
            <div class="seg flex-wrap" role="group" aria-labelledby="role-access-label">
              {#each accessChoices as choice (choice.id)}
                <button
                  type="button"
                  class="seg-item"
                  aria-pressed={form.access.level === choice.id}
                  onclick={() => (form.access.level = choice.id)}
                >
                  {choice.label}
                </button>
              {/each}
            </div>
          </div>
          <span class="text-xs muted">
            {t("Esquema de la base {database}", { database: database || "—" })}.
            {t("Los permisos de otras bases se dan estando conectado a ellas.")}
          </span>
          {#if form.access.level !== "none"}
            <label class="check items-start">
              <input class="mt-0.5" type="checkbox" bind:checked={form.access.futureTables} />
              <span>
                {t("También las tablas que se creen después")}
                <span class="block text-xs muted">
                  {t("Solo vale para las tablas que cree el rol que ejecuta esto, no las de otros.")}
                </span>
              </span>
            </label>
          {/if}
        </div>
      {/if}

      <details class="rounded-md border border-zinc-200 px-3 py-2 dark:border-zinc-700">
        <summary class="cursor-pointer text-sm font-medium focus-visible:outline-2 focus-visible:outline-blue-500">
          {t("Avanzado")}
        </summary>
        <div class="mt-3 flex flex-col gap-2.5">
          {#if existing}
            <label class="check items-start">
              <input class="mt-0.5" type="checkbox" bind:checked={form.login} />
              <span>
                {t("Puede iniciar sesión")}
                <span class="block text-xs muted">{t("LOGIN: sin esto el rol solo sirve como grupo.")}</span>
              </span>
            </label>
          {/if}
          {#each attributes as info (info.key)}
            <label class="check items-start">
              <input class="mt-0.5" type="checkbox" bind:checked={form[info.key]} />
              <span class={info.risk === "high" && form[info.key] ? "text-rose-600 dark:text-rose-400" : ""}>
                {info.label}
                <span
                  class="block text-xs {info.risk === 'high'
                    ? 'text-amber-700 dark:text-amber-400'
                    : 'muted'}"
                >
                  {info.help}
                </span>
              </span>
            </label>
          {/each}
          <label class="check items-start">
            <input class="mt-0.5" type="checkbox" bind:checked={form.adminOption} />
            <span>
              {t("Puede dar a otros el acceso al grupo (ADMIN OPTION)")}
              <span class="block text-xs muted">
                {t("Vale para los grupos que se agregan ahora, no para los que ya tenía.")}
              </span>
            </span>
          </label>
          {#if isUser}
            <label class="flex flex-col gap-1">
              <span class="label">{t("Límite de conexiones")}</span>
              <input
                class="field max-w-40"
                type="number"
                min="0"
                step="1"
                bind:value={form.connectionLimit}
                placeholder={t("sin límite")}
              />
            </label>
          {/if}
        </div>
      </details>

      {#if error}
        <Alert tone="bad" box>{error}</Alert>
      {/if}
    </div>

    <aside class="flex min-w-0 flex-col gap-3" aria-label={t("Qué va a pasar")}>
      <section class="card p-3">
        <h3 class="card-title mb-2">{t("Qué va a pasar")}</h3>
        {#if existing}
          {@const count = changes().length}
          <p class="text-xs">
            {count === 0
              ? t("Todavía no hay cambios.")
              : t("Se aplican {n} sentencias al rol {name}.", { n: count, name: existing.name })}
          </p>
        {:else}
          <ul class="list-disc space-y-1 pl-4 text-xs">
            {#each summary as line (line)}
              <li>{line}</li>
            {/each}
          </ul>
        {/if}
      </section>

      {#if risky.length > 0}
        <Alert tone="bad" box>
          {#if form.superuser}
            <b>{t("Superusuario.")}</b>
            {t("Este rol saltará todos los permisos del servidor. En producción pedirá escribir el nombre del servidor para confirmar.")}
          {:else}
            {t("Este rol tiene atributos de riesgo: {list}.", { list: risky.map((r) => r.label).join(", ") })}
          {/if}
        </Alert>
      {/if}

      <section class="card p-3">
        <h3 class="card-title">{t("SQL que se ejecuta")}</h3>
        {#if liveSql}
          <SqlPreview sql={liveSql} {profileId} {database} />
        {:else if liveFailure}
          <p class="mt-2 text-xs text-rose-600 dark:text-rose-400">{liveFailure}</p>
        {:else}
          <p class="mt-2 text-xs muted">
            {validate() ?? (loadingMemberships ? t("Leyendo las membresías…") : t("Nada que aplicar."))}
          </p>
        {/if}
      </section>
    </aside>
  </div>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose} disabled={saving}>{t("Cancelar")}</button>
    <button class="btn btn-primary" onclick={submit} disabled={saving}>
      {#if saving}<span class="spinner"></span>{/if}
      {submitLabel}
    </button>
  {/snippet}
</Modal>
