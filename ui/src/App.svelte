<script lang="ts">
  import ConnectionDialog from "./lib/ConnectionDialog.svelte";
  import Dashboard from "./lib/Dashboard.svelte";
  import DetailPanel from "./lib/DetailPanel.svelte";
  import TreePanel from "./lib/TreePanel.svelte";
  import { explorer } from "./lib/explorer.svelte";
  import {
    appInfo,
    deleteProfile,
    describeError,
    formatVersion,
    type AppInfo,
    type ConnectionProfile,
  } from "./lib/ipc";

  let info = $state<AppInfo | null>(null);
  let dialog = $state<{ profile: ConnectionProfile | null } | null>(null);
  let prompt = $state<{ profile: ConnectionProfile; message: string; password: string } | null>(
    null,
  );
  let banner = $state<string | null>(null);
  let sidebarWidth = $state(320);
  let view = $state<"explorer" | "monitor">("explorer");
  /** Servidor sobre el que trabaja el monitoreo. */
  let activeServer = $state<string | null>(null);

  $effect(() => {
    appInfo().then((value) => (info = value));
    explorer.refreshProfiles().catch((e) => (banner = describeError(e)));
  });

  async function connect(profile: ConnectionProfile, password?: string) {
    banner = null;
    try {
      await explorer.connect(profile, password);
      activeServer = profile.id;
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

  async function remove(profile: ConnectionProfile) {
    try {
      await deleteProfile(profile.id);
      explorer.roots = explorer.roots.filter((row) => row.profileId !== profile.id);
      await explorer.refreshProfiles();
    } catch (error) {
      banner = describeError(error);
    }
  }

  function startResize(event: MouseEvent) {
    event.preventDefault();
    const move = (e: MouseEvent) => {
      sidebarWidth = Math.min(640, Math.max(220, e.clientX));
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
  <header
    class="flex items-center gap-3 border-b border-neutral-200 px-3 py-2 dark:border-neutral-800"
  >
    <span class="font-semibold tracking-tight">pgforge</span>
    {#if info}
      <span class="text-xs text-neutral-400">v{info.version}</span>
    {/if}

    <button class="btn btn-primary ml-2" onclick={() => (dialog = { profile: null })}>
      Nuevo servidor
    </button>

    <nav class="ml-4 flex gap-3">
      {#each [["explorer", "Explorador"], ["monitor", "Monitoreo"]] as [value, label] (value)}
        <button
          class="text-sm {view === value
            ? 'font-medium text-blue-600 dark:text-blue-400'
            : 'text-neutral-500 hover:text-neutral-900 dark:hover:text-neutral-100'}"
          onclick={() => (view = value as typeof view)}
        >
          {label}
        </button>
      {/each}
    </nav>

    <label
      class="ml-auto flex items-center gap-1.5 text-xs text-neutral-500"
      class:hidden={view !== "explorer"}
    >
      <input
        type="checkbox"
        checked={explorer.options.showSystemSchemas}
        onchange={(event) => {
          explorer.options = {
            showSystemSchemas: (event.currentTarget as HTMLInputElement).checked,
          };
          explorer.reloadAll();
        }}
      />
      Mostrar objetos del sistema
    </label>
  </header>

  {#if banner}
    <div
      class="flex items-center gap-2 border-b border-red-200 bg-red-50 px-3 py-1.5 text-sm
             text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300"
    >
      <span class="flex-1">{banner}</span>
      <button class="text-xs underline" onclick={() => (banner = null)}>cerrar</button>
    </div>
  {/if}

  {#if view === "monitor"}
    {#if activeServer && explorer.isConnected(activeServer)}
      <div class="min-h-0 flex-1">
        {#key activeServer}
          <Dashboard profileId={activeServer} />
        {/key}
      </div>
    {:else}
      <div class="flex flex-1 items-center justify-center p-6 text-sm text-neutral-500">
        Conectá un servidor para monitorearlo.
      </div>
    {/if}
  {:else}
  <div class="flex min-h-0 flex-1">
    <aside class="flex min-h-0 flex-col" style="width: {sidebarWidth}px">
      <div class="max-h-56 overflow-auto border-b border-neutral-200 dark:border-neutral-800">
        {#if explorer.profiles.length === 0}
          <p class="p-3 text-xs text-neutral-500">Todavía no hay servidores guardados.</p>
        {/if}
        {#each explorer.profiles as profile (profile.id)}
          {@const connected = explorer.isConnected(profile.id)}
          {@const caps = explorer.caps[profile.id]}
          <div class="group flex items-center gap-2 px-3 py-1.5 text-sm">
            <span
              class="size-1.5 shrink-0 rounded-full {connected
                ? 'bg-emerald-500'
                : 'bg-neutral-300 dark:bg-neutral-600'}"
              title={connected ? "Conectado" : "Desconectado"}
            ></span>
            <button
              class="truncate {activeServer === profile.id ? 'font-medium' : ''}"
              title="Elegir como servidor activo"
              onclick={() => (activeServer = profile.id)}
            >
              {profile.name}
            </button>
            <span class="truncate text-xs text-neutral-400">
              {#if connected && caps}
                PostgreSQL {formatVersion(caps.version)}
              {:else}
                {profile.host}:{profile.port}
              {/if}
            </span>
            <span class="ml-auto flex shrink-0 gap-1 opacity-0 group-hover:opacity-100">
              {#if connected}
                <button class="link" onclick={() => explorer.disconnect(profile.id)}>
                  desconectar
                </button>
              {:else}
                <button class="link" onclick={() => connect(profile)}>conectar</button>
              {/if}
              <button class="link" onclick={() => (dialog = { profile })}>editar</button>
              <button class="link" onclick={() => remove(profile)}>borrar</button>
            </span>
          </div>
        {/each}
      </div>

      <div class="min-h-0 flex-1">
        <TreePanel />
      </div>
    </aside>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="w-px shrink-0 cursor-col-resize bg-neutral-200 hover:bg-blue-400 dark:bg-neutral-800"
      onmousedown={startResize}
    ></div>

    <main class="min-w-0 flex-1">
      <DetailPanel />
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
  <div class="fixed inset-0 z-10 flex items-center justify-center bg-black/40 p-4">
    <div class="w-full max-w-sm rounded-lg bg-white p-5 shadow-xl dark:bg-neutral-900">
      <h2 class="text-base font-medium">Contraseña de {prompt.profile.name}</h2>
      <p class="mt-1 text-xs text-neutral-500">{prompt.message}</p>
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
