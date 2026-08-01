<script lang="ts">
  import ConnectionDialog from "./lib/ConnectionDialog.svelte";
  import Dashboard from "./lib/Dashboard.svelte";
  import DetailPanel from "./lib/DetailPanel.svelte";
  import Icon from "./lib/Icon.svelte";
  import TreePanel from "./lib/TreePanel.svelte";
  import { explorer } from "./lib/explorer.svelte";
  import {
    appInfo,
    deleteProfile,
    describeError,
    type AppInfo,
    type ConnectionProfile,
  } from "./lib/ipc";

  let info = $state<AppInfo | null>(null);
  let dialog = $state<{ profile: ConnectionProfile | null } | null>(null);
  let prompt = $state<{ profile: ConnectionProfile; message: string; password: string } | null>(
    null,
  );
  let confirmDelete = $state<ConnectionProfile | null>(null);
  let banner = $state<string | null>(null);
  let sidebarWidth = $state(300);
  let view = $state<"explorer" | "monitor">("explorer");
  /** Servidor elegido a mano en la vista de monitoreo; si es `null` se usa el del árbol. */
  let monitorChoice = $state<string | null>(null);

  $effect(() => {
    appInfo().then((value) => (info = value));
    explorer.refreshProfiles().catch((error) => (banner = describeError(error)));
  });

  const connectedServers = $derived(explorer.roots.filter((row) => row.connected));

  const monitorServer = $derived.by(() => {
    if (monitorChoice && explorer.isConnected(monitorChoice)) return monitorChoice;
    const selected = explorer.selected;
    if (selected && explorer.isConnected(selected.profileId)) return selected.profileId;
    return connectedServers[0]?.profileId ?? null;
  });

  function profileOf(profileId: string) {
    return explorer.profiles.find((profile) => profile.id === profileId) ?? null;
  }

  async function connect(profile: ConnectionProfile, password?: string) {
    banner = null;
    try {
      await explorer.connect(profile, password);
      prompt = null;
    } catch (error) {
      const message = describeError(error);
      // Un fallo de autenticación no es un error para mostrar y olvidar: es una pregunta.
      if (/contrase|password|authentication|autenticaci/i.test(message)) {
        prompt = { profile, message, password: "" };
      } else {
        banner = message;
      }
    }
  }

  function connectById(profileId: string) {
    const profile = profileOf(profileId);
    if (profile) connect(profile);
  }

  async function remove(profile: ConnectionProfile) {
    confirmDelete = null;
    try {
      await deleteProfile(profile.id);
      await explorer.refreshProfiles();
      if (explorer.selected?.profileId === profile.id) explorer.selected = null;
    } catch (error) {
      banner = describeError(error);
    }
  }

  function startResize(event: MouseEvent) {
    event.preventDefault();
    const move = (moved: MouseEvent) => {
      sidebarWidth = Math.min(560, Math.max(220, moved.clientX));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }
</script>

<div class="flex h-full flex-col">
  <header class="divider-b flex items-center gap-3 px-3 py-2">
    <div class="flex items-center gap-2">
      <span
        class="grid size-6 place-items-center rounded-md bg-blue-600 font-mono text-[11px]
               font-bold text-white">pg</span
      >
      <span class="text-sm font-semibold tracking-tight">pgforge</span>
    </div>

    <div class="seg ml-2" role="tablist">
      {#each [["explorer", "Explorador"], ["monitor", "Monitoreo"]] as [value, label] (value)}
        <button
          class="seg-item"
          role="tab"
          aria-selected={view === value}
          onclick={() => (view = value as typeof view)}
        >
          {label}
        </button>
      {/each}
    </div>

    {#if view === "monitor" && connectedServers.length > 0}
      <select
        class="field w-44 py-0.5 text-xs"
        title="Servidor que se está monitoreando"
        value={monitorServer}
        onchange={(event) => (monitorChoice = event.currentTarget.value)}
      >
        {#each connectedServers as server (server.profileId)}
          <option value={server.profileId}>{server.label}</option>
        {/each}
      </select>
    {/if}

    {#if info}
      <span class="ml-auto text-xs muted">v{info.version}</span>
    {/if}
  </header>

  {#if banner}
    <div
      class="flex items-center gap-2 border-b border-rose-200 bg-rose-50 px-3 py-1.5 text-sm
             text-rose-700 dark:border-rose-900 dark:bg-rose-950 dark:text-rose-300"
    >
      <span class="flex-1">{banner}</span>
      <button class="btn btn-ghost px-1.5 py-0.5" onclick={() => (banner = null)}>
        <Icon name="close" size={12} />
      </button>
    </div>
  {/if}

  {#if view === "monitor"}
    {#if monitorServer}
      <div class="min-h-0 flex-1">
        {#key monitorServer}
          <Dashboard profileId={monitorServer} />
        {/key}
      </div>
    {:else}
      <div class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
        <Icon name="server" size={28} class="text-zinc-300 dark:text-zinc-700" />
        <p class="text-sm muted">Conectá un servidor para monitorearlo.</p>
        <button class="btn" onclick={() => (view = "explorer")}>Ir al explorador</button>
      </div>
    {/if}
  {:else}
    <div class="flex min-h-0 flex-1">
      <aside class="panel divider-r flex min-h-0 flex-col" style="width: {sidebarWidth}px">
        <div class="flex items-center gap-1.5 px-2 py-2">
          <div class="relative flex-1">
            <Icon
              name="search"
              size={13}
              class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-zinc-400"
            />
            <input
              class="field w-full py-1 pl-7"
              placeholder="Buscar"
              title="Busca entre los objetos ya cargados en el árbol"
              bind:value={explorer.search}
            />
          </div>
          <button
            class="btn btn-icon"
            title="Nuevo servidor"
            aria-label="Nuevo servidor"
            onclick={() => (dialog = { profile: null })}
          >
            <Icon name="plus" />
          </button>
        </div>

        <div class="min-h-0 flex-1 px-1 pb-1">
          <TreePanel onconnect={connectById} />
        </div>

        <div class="divider-t px-3 py-2">
          <label class="check">
            <input
              type="checkbox"
              checked={explorer.options.showSystemSchemas}
              onchange={(event) => {
                explorer.options = { showSystemSchemas: event.currentTarget.checked };
                explorer.reloadAll();
              }}
            />
            Mostrar objetos del sistema
          </label>
        </div>
      </aside>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="w-px shrink-0 cursor-col-resize bg-zinc-200 transition-colors hover:bg-blue-400
               dark:bg-zinc-800"
        onmousedown={startResize}
      ></div>

      <main class="min-w-0 flex-1">
        <DetailPanel
          onconnect={connectById}
          onedit={(profileId) => {
            const profile = profileOf(profileId);
            if (profile) dialog = { profile };
          }}
          ondelete={(profileId) => (confirmDelete = profileOf(profileId))}
        />
      </main>
    </div>
  {/if}
</div>

{#if dialog}
  <ConnectionDialog
    profile={dialog.profile}
    onclose={() => (dialog = null)}
    onsaved={async (profile, password) => {
      dialog = null;
      await explorer.refreshProfiles();
      if (password !== undefined || profile.savePassword) {
        await connect(profile, password);
      }
    }}
  />
{/if}

{#if prompt}
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
    <div class="card w-full max-w-sm p-5 shadow-xl">
      <h2 class="text-base font-medium">Contraseña de {prompt.profile.name}</h2>
      <p class="mt-1 text-xs muted">{prompt.message}</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="field mt-3 w-full"
        type="password"
        autocomplete="off"
        autofocus
        bind:value={prompt.password}
        onkeydown={(event) => {
          if (event.key === "Enter" && prompt) connect(prompt.profile, prompt.password);
        }}
      />
      <div class="mt-4 flex justify-end gap-2">
        <button class="btn" onclick={() => (prompt = null)}>Cancelar</button>
        <button
          class="btn btn-primary"
          onclick={() => prompt && connect(prompt.profile, prompt.password)}
        >
          Conectar
        </button>
      </div>
    </div>
  </div>
{/if}

{#if confirmDelete}
  <div class="fixed inset-0 z-10 grid place-items-center bg-black/40 p-4">
    <div class="card w-full max-w-sm p-5 shadow-xl">
      <h2 class="text-base font-medium">Eliminar «{confirmDelete.name}»</h2>
      <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">
        Se borra el servidor de la lista y su contraseña guardada. No se toca nada en la base de
        datos.
      </p>
      <div class="mt-4 flex justify-end gap-2">
        <button class="btn" onclick={() => (confirmDelete = null)}>Cancelar</button>
        <button
          class="btn btn-primary border-rose-600 bg-rose-600 hover:bg-rose-700"
          onclick={() => confirmDelete && remove(confirmDelete)}
        >
          Eliminar
        </button>
      </div>
    </div>
  </div>
{/if}
