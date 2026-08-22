<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import SettingDialog from "./SettingDialog.svelte";
  import { explorer } from "./explorer.svelte";
  import { describeError, serverSettings, type Setting } from "./ipc";

  let { profileId }: { profileId: string } = $props();

  let settings = $state<Setting[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let search = $state("");
  let editing = $state<Setting | null>(null);
  let flash = $state<string | null>(null);

  const isSuperuser = $derived(explorer.caps[profileId]?.isSuperuser ?? false);
  const pendingCount = $derived(settings.filter((setting) => setting.pendingRestart).length);

  function load() {
    loading = true;
    error = null;
    serverSettings(profileId)
      .then((result) => (settings = result))
      .catch((e) => (error = describeError(e)))
      .finally(() => (loading = false));
  }

  $effect(() => {
    void profileId;
    load();
  });

  const filtered = $derived.by(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return settings;
    return settings.filter(
      (setting) =>
        setting.name.toLowerCase().includes(needle) ||
        setting.shortDesc.toLowerCase().includes(needle),
    );
  });

  // Agrupadas por categoría, respetando el orden (ya vienen ordenadas por categoría del servidor).
  const groups = $derived.by(() => {
    const map = new Map<string, Setting[]>();
    for (const setting of filtered) {
      const list = map.get(setting.category);
      if (list) list.push(setting);
      else map.set(setting.category, [setting]);
    }
    return [...map.entries()];
  });

  /** Solo se puede editar si el rol es superusuario y el parámetro no es de solo lectura. */
  function editable(setting: Setting): boolean {
    return isSuperuser && setting.context !== "internal";
  }

  function open(setting: Setting) {
    if (editable(setting)) editing = setting;
  }

  function afterSaved(pendingRestart: boolean) {
    editing = null;
    flash = pendingRestart
      ? "Guardado. El cambio necesita reiniciar el servidor para tomar efecto."
      : "Guardado y recargado.";
    load();
  }
</script>

<div class="flex h-full flex-col">
  <div class="divider-b flex flex-wrap items-center gap-3 px-3 py-2">
    <div class="relative">
      <Icon
        name="search"
        size={13}
        class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-zinc-400"
      />
      <input
        class="field w-64 py-1 pr-3 pl-7"
        placeholder="Buscar un parámetro"
        bind:value={search}
      />
    </div>

    <span class="text-xs muted">{filtered.length} de {settings.length} parámetros</span>

    <button class="btn btn-sm ml-auto" onclick={load} disabled={loading}>
      <Icon name="refresh" size={11} />
      Recargar
    </button>
  </div>

  {#if !isSuperuser}
    <Alert tone="warn">
      Solo lectura: hace falta ser superusuario para cambiar la configuración con ALTER SYSTEM.
    </Alert>
  {/if}

  {#if pendingCount > 0}
    <Alert tone="warn">
      Hay {pendingCount}
      {pendingCount === 1 ? "parámetro" : "parámetros"} pendientes de un reinicio del servidor para tomar
      efecto.
    </Alert>
  {/if}

  {#if flash}
    <Alert tone="ok" onclose={() => (flash = null)}>{flash}</Alert>
  {/if}

  {#if error}
    <Alert tone="bad">{error}</Alert>
  {/if}

  <div class="min-h-0 flex-1 overflow-auto px-3 py-2">
    {#if loading}
      <div class="flex items-center gap-2 text-sm muted"><span class="spinner"></span> Leyendo…</div>
    {:else if filtered.length === 0}
      <Empty icon="search" title="Sin coincidencias" hint="Probá con otro nombre de parámetro." />
    {:else}
      {#each groups as [category, items] (category)}
        <div class="label mt-3 mb-1 first:mt-0">{category}</div>
        <div class="card divide-y divide-zinc-100 overflow-hidden dark:divide-zinc-700">
          {#each items as setting (setting.name)}
            {@const canEdit = editable(setting)}
            <button
              type="button"
              class="flex w-full items-baseline gap-3 px-3 py-1.5 text-left text-sm
                     enabled:hover:bg-zinc-100 enabled:hover:cursor-pointer
                     dark:enabled:hover:bg-zinc-800/70"
              disabled={!canEdit}
              onclick={() => open(setting)}
            >
              <span class="w-72 shrink-0 truncate font-mono text-xs" title={setting.name}>
                {setting.name}
              </span>
              <!-- Sin valor no es un parámetro vacío: es uno que este rol no tiene permiso de leer. -->
              <span
                class="w-40 shrink-0 truncate tabular-nums {setting.value === null ? 'muted' : ''}"
                title={setting.value ?? "solo lo puede leer un superusuario"}
              >
                {#if setting.value === null}
                  sin permiso
                {:else}
                  {setting.value}{setting.unit ? ` ${setting.unit}` : ""}
                {/if}
              </span>

              <span class="flex shrink-0 gap-1">
                {#if setting.pendingRestart}
                  <span class="tag tag-warn">pendiente de reinicio</span>
                {:else if setting.context === "postmaster"}
                  <span class="tag tag-neutral">necesita reinicio</span>
                {:else if setting.context === "internal"}
                  <span class="tag tag-neutral">solo lectura</span>
                {/if}
                {#if setting.source !== "default" && setting.context !== "internal"}
                  <span class="tag tag-info" title="Distinto del valor por omisión">cambiado</span>
                {/if}
              </span>

              <span class="min-w-0 flex-1 truncate text-xs muted" title={setting.shortDesc}>
                {setting.shortDesc}
              </span>

              {#if canEdit}
                <Icon name="edit" size={12} class="shrink-0 text-zinc-400" />
              {/if}
            </button>
          {/each}
        </div>
      {/each}
    {/if}
  </div>
</div>

{#if editing}
  <SettingDialog
    {profileId}
    setting={editing}
    onclose={() => (editing = null)}
    onsaved={afterSaved}
  />
{/if}
