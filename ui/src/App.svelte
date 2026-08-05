<script lang="ts">
  import Alert from "./lib/Alert.svelte";
  import Confirm from "./lib/Confirm.svelte";
  import ConnectionDialog from "./lib/ConnectionDialog.svelte";
  import GroupDialog from "./lib/GroupDialog.svelte";
  import NewGroupDialog from "./lib/NewGroupDialog.svelte";
  import Dashboard from "./lib/Dashboard.svelte";
  import ServerConfig from "./lib/ServerConfig.svelte";
  import DataPanel from "./lib/DataPanel.svelte";
  import DetailPanel from "./lib/DetailPanel.svelte";
  import ErdPanel from "./lib/ErdPanel.svelte";
  import Empty from "./lib/Empty.svelte";
  import Icon, { type IconName } from "./lib/Icon.svelte";
  import Modal from "./lib/Modal.svelte";
  import QueryPanel from "./lib/QueryPanel.svelte";
  import TreePanel from "./lib/TreePanel.svelte";
  import { openData, DataTab } from "./lib/data.svelte";
  import { openErd, ErdTab } from "./lib/erd.svelte";
  import { explorer } from "./lib/explorer.svelte";
  import { openQuery, QueryTab } from "./lib/query.svelte";
  import { tabs, type TabKind } from "./lib/tabs.svelte";
  import { theme } from "./lib/theme.svelte";
  import {
    appInfo,
    deleteProfile,
    describeError,
    formatVersion,
    sshHostKey,
    type AppInfo,
    type ConnectionProfile,
  } from "./lib/ipc";

  let info = $state<AppInfo | null>(null);
  let dialog = $state<{ profile: ConnectionProfile | null } | null>(null);
  let prompt = $state<{ profile: ConnectionProfile; message: string; password: string } | null>(
    null,
  );
  let confirmDelete = $state<ConnectionProfile | null>(null);
  /**
   * Confirmación de la clave del host de un bastión SSH sin verificar. Guarda la contraseña con la
   * que se estaba conectando para reintentar tal cual una vez que el usuario acepta la huella.
   */
  let hostKey = $state<{
    profile: ConnectionProfile;
    host: string;
    fingerprint: string;
    changed: boolean;
    password?: string;
  } | null>(null);
  /** La carpeta de conexiones que se está renombrando. */
  let groupDialog = $state<string | null>(null);
  /** Abierto mientras se crea una carpeta nueva. */
  let newGroupDialog = $state(false);
  let banner = $state<string | null>(null);
  let sidebarWidth = $state(300);
  let sidebarOpen = $state(true);
  let view = $state<"explorer" | "monitor" | "config">("explorer");
  /** Servidor elegido a mano en la vista de monitoreo; si es `null` se usa el del árbol. */
  let monitorChoice = $state<string | null>(null);
  /** Servidor elegido a mano en la vista de configuración. */
  let configChoice = $state<string | null>(null);

  const DEFAULT_SIDEBAR = 300;

  const TAB_ICON: Record<TabKind, IconName> = {
    query: "sql",
    data: "table",
    erd: "diagram",
  };

  $effect(() => {
    appInfo().then((value) => (info = value));
    explorer.refreshProfiles().catch((error) => (banner = describeError(error)));
  });

  const connectedServers = $derived(explorer.servers.filter((row) => row.connected));

  const monitorServer = $derived.by(() => {
    if (monitorChoice && explorer.isConnected(monitorChoice)) return monitorChoice;
    const selected = explorer.selected;
    if (selected && explorer.isConnected(selected.profileId)) return selected.profileId;
    return connectedServers[0]?.profileId ?? null;
  });

  const configServer = $derived.by(() => {
    if (configChoice && explorer.isConnected(configChoice)) return configChoice;
    const selected = explorer.selected;
    if (selected && explorer.isConnected(selected.profileId)) return selected.profileId;
    return connectedServers[0]?.profileId ?? null;
  });

  function profileOf(profileId: string) {
    return explorer.profiles.find((profile) => profile.id === profileId) ?? null;
  }

  async function connect(profile: ConnectionProfile, password?: string, trustHostKey?: boolean) {
    banner = null;
    try {
      await explorer.connect(profile, password, trustHostKey);
      prompt = null;
      hostKey = null;
    } catch (error) {
      // Una clave de host SSH sin verificar no es una falla: es una pregunta de seguridad. Se
      // muestra la huella y, si el usuario confía, se reintenta la misma conexión aceptándola.
      const host = sshHostKey(error);
      if (host) {
        hostKey = { profile, password, ...host };
        return;
      }
      const message = describeError(error);
      // Un fallo de autenticación tampoco es un error para mostrar y olvidar: es una pregunta.
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

  // Una pestaña vive sobre una conexión: si el servidor se desconecta, no queda nada del otro
  // lado. Se cierra acá y no en cada lugar que desconecta, para que ningún camino se olvide.
  $effect(() => {
    const connected = new Set(
      explorer.servers.filter((row) => row.connected).map((row) => row.profileId),
    );
    for (const tab of tabs.all) {
      if (!connected.has(tab.profileId)) tabs.close(tab.key);
    }
  });

  function startResize(event: MouseEvent) {
    event.preventDefault();
    const move = (moved: MouseEvent) => {
      sidebarWidth = Math.min(560, Math.max(220, moved.clientX));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("cursor-col-resize");
    };
    // Mientras se arrastra, el cursor no cambia al pasar por encima de otros elementos.
    document.body.classList.add("cursor-col-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  /** Atajos que valen en toda la ventana. Los del editor los maneja CodeMirror, que tiene el foco. */
  function onKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "b") {
      event.preventDefault();
      sidebarOpen = !sidebarOpen;
    }
  }

  const THEME_LABEL = {
    system: "Tema: el del sistema",
    light: "Tema: claro",
    dark: "Tema: oscuro",
  } as const;

  const THEME_ICON = { system: "auto", light: "sun", dark: "moon" } as const;

  const VIEWS = [
    { value: "explorer", label: "Explorador", icon: "schema" },
    { value: "monitor", label: "Monitoreo", icon: "chart" },
    { value: "config", label: "Configuración", icon: "sliders" },
  ] as const;

  /** Lo que dice la barra de estado: dónde está parado el usuario ahora mismo. */
  const context = $derived.by(() => {
    const selected = explorer.selected;
    if (!selected) return null;
    if (selected.kind === "group") {
      return { server: selected.label, connected: false, version: null, path: "carpeta" };
    }
    const profile = profileOf(selected.profileId);
    const caps = explorer.caps[selected.profileId];
    return {
      server: profile?.name ?? selected.label,
      connected: explorer.isConnected(selected.profileId),
      version: caps ? `PostgreSQL ${formatVersion(caps.version)}` : null,
      path: selected.node
        ? [selected.node.database, selected.node.schema, selected.node.label]
            .filter(Boolean)
            .join(" / ")
        : (profile?.host ?? ""),
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full flex-col">
  <header class="divider-b flex items-center gap-3 px-3 py-2">
    <div class="flex items-center gap-2">
      <span
        class="grid size-6 place-items-center rounded-md bg-blue-600 font-mono text-[11px]
               font-bold text-white shadow-sm shadow-blue-600/30">pg</span
      >
      <span class="text-sm font-semibold tracking-tight">pgforge</span>
    </div>

    <div class="seg ml-2" role="tablist">
      {#each VIEWS as item (item.value)}
        <button
          class="seg-item"
          role="tab"
          aria-selected={view === item.value}
          onclick={() => (view = item.value)}
        >
          <Icon name={item.icon} size={12} />
          {item.label}
        </button>
      {/each}
    </div>

    {#if view === "monitor" && connectedServers.length > 0}
      <label class="check gap-1.5">
        Servidor
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
      </label>
    {/if}

    {#if view === "config" && connectedServers.length > 0}
      <label class="check gap-1.5">
        Servidor
        <select
          class="field w-44 py-0.5 text-xs"
          title="Servidor cuya configuración se está viendo"
          value={configServer}
          onchange={(event) => (configChoice = event.currentTarget.value)}
        >
          {#each connectedServers as server (server.profileId)}
            <option value={server.profileId}>{server.label}</option>
          {/each}
        </select>
      </label>
    {/if}

    <div class="ml-auto flex items-center gap-2">
      {#if connectedServers.length > 0}
        <span class="flex items-center gap-1.5 text-xs muted" title="Servidores conectados">
          <span class="dot dot-on"></span>
          {connectedServers.length}
          {connectedServers.length === 1 ? "conectado" : "conectados"}
        </span>
      {/if}

      <button
        class="btn btn-ghost btn-icon"
        title={THEME_LABEL[theme.preference]}
        aria-label={THEME_LABEL[theme.preference]}
        onclick={() => theme.cycle()}
      >
        <Icon name={THEME_ICON[theme.preference]} size={15} />
      </button>

      {#if info}
        <span class="text-xs muted">v{info.version}</span>
      {/if}
    </div>
  </header>

  {#if banner}
    <Alert tone="bad" onclose={() => (banner = null)}>{banner}</Alert>
  {/if}

  {#if view === "monitor"}
    {#if monitorServer}
      <div class="min-h-0 flex-1">
        {#key monitorServer}
          <Dashboard profileId={monitorServer} />
        {/key}
      </div>
    {:else}
      <Empty
        icon="server"
        title="No hay ningún servidor conectado"
        hint="El monitoreo lee las estadísticas en vivo de una conexión abierta."
      >
        <button class="btn btn-primary" onclick={() => (view = "explorer")}>
          Ir al explorador
        </button>
      </Empty>
    {/if}
  {:else if view === "config"}
    {#if configServer}
      <div class="min-h-0 flex-1">
        {#key configServer}
          <ServerConfig profileId={configServer} />
        {/key}
      </div>
    {:else}
      <Empty
        icon="server"
        title="No hay ningún servidor conectado"
        hint="La configuración se lee de una conexión abierta."
      >
        <button class="btn btn-primary" onclick={() => (view = "explorer")}>
          Ir al explorador
        </button>
      </Empty>
    {/if}
  {:else}
    <div class="flex min-h-0 flex-1">
      {#if sidebarOpen}
        <aside class="panel flex min-h-0 flex-col" style="width: {sidebarWidth}px">
          <div class="flex items-center gap-1.5 px-2 py-2">
            <div class="relative flex-1">
              <Icon
                name="search"
                size={13}
                class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-zinc-400"
              />
              <input
                class="field w-full py-1 pr-7 pl-7"
                placeholder="Buscar"
                title="Busca entre los objetos ya cargados en el árbol"
                bind:value={explorer.search}
                onkeydown={(event) => {
                  if (event.key === "Escape") explorer.search = "";
                }}
              />
              {#if explorer.search}
                <button
                  class="btn btn-ghost btn-icon absolute top-1/2 right-0.5 size-6 -translate-y-1/2"
                  aria-label="Limpiar la búsqueda"
                  onclick={() => (explorer.search = "")}
                >
                  <Icon name="close" size={11} />
                </button>
              {/if}
            </div>
            <button
              class="btn btn-icon"
              title="Nueva carpeta"
              aria-label="Nueva carpeta"
              onclick={() => (newGroupDialog = true)}
            >
              <Icon name="folder" />
            </button>
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
            <TreePanel
              onconnect={connectById}
              onnew={() => (dialog = { profile: null })}
              ongroup={(name) => (groupDialog = name)}
            />
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

        <!--
          El separador mide un píxel a la vista pero atrapa el mouse en seis: agarrar una línea de
          un píxel es de las cosas más frustrantes de una interfaz de escritorio.
        -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="group relative w-px shrink-0 bg-zinc-200 dark:bg-zinc-800"
          onmousedown={startResize}
          ondblclick={() => (sidebarWidth = DEFAULT_SIDEBAR)}
          title="Arrastrá para cambiar el ancho; doble clic para restablecerlo"
        >
          <div
            class="absolute inset-y-0 -left-[3px] w-[7px] cursor-col-resize
                   transition-colors group-hover:bg-blue-500/40"
          ></div>
        </div>
      {/if}

      <main class="flex min-w-0 flex-1 flex-col">
        <!-- `overflow-y-hidden`: sin eso, el subrayado de la pestaña activa desborda un píxel hacia
             abajo y el navegador dibuja una barra de desplazamiento vertical en toda la tira. -->
        <div
          class="divider-b flex items-stretch gap-px overflow-x-auto overflow-y-hidden px-1 pt-1"
          role="tablist"
        >
          <button
            class="btn btn-ghost btn-icon mr-1 self-center"
            title={sidebarOpen ? "Ocultar el árbol (Ctrl+B)" : "Mostrar el árbol (Ctrl+B)"}
            aria-label={sidebarOpen ? "Ocultar el árbol" : "Mostrar el árbol"}
            onclick={() => (sidebarOpen = !sidebarOpen)}
          >
            <Icon
              name="chevron"
              size={13}
              class="transition-transform {sidebarOpen ? 'rotate-180' : ''}"
            />
          </button>

          <div class="tab-wrap">
            <button
              class="tab"
              role="tab"
              aria-selected={tabs.active === null}
              title="El objeto seleccionado en el árbol"
              onclick={() => (tabs.active = null)}
            >
              <Icon name="compass" size={12} class="muted" />
              Detalle
            </button>
          </div>

          {#each tabs.all as tab (tab.key)}
            <div class="tab-wrap">
              <button
                class="tab pr-1"
                role="tab"
                aria-selected={tabs.active === tab.key}
                title={`${tab.title} · ${tab.database}`}
                onclick={() => (tabs.active = tab.key)}
                onauxclick={(event) => {
                  // Botón del medio: cerrar, como en cualquier navegador.
                  if (event.button === 1) tabs.close(tab.key);
                }}
              >
                {#if tab instanceof QueryTab && tab.running}
                  <span class="spinner"></span>
                {:else}
                  <Icon name={TAB_ICON[tab.kind]} size={12} class="muted" />
                {/if}
                <span class="truncate">{tab.title}</span>
              </button>
              <button
                class="tab-close"
                aria-label="Cerrar la pestaña"
                title="Cerrar la pestaña"
                onclick={() => tabs.close(tab.key)}
              >
                <Icon name="close" size={10} />
              </button>
            </div>
          {/each}
        </div>

        <div class="min-h-0 flex-1">
          {#if tabs.current instanceof QueryTab}
            {#key tabs.current.key}
              <QueryPanel tab={tabs.current} />
            {/key}
          {:else if tabs.current instanceof DataTab}
            {#key tabs.current.key}
              <DataPanel tab={tabs.current} />
            {/key}
          {:else if tabs.current instanceof ErdTab}
            {#key tabs.current.key}
              <ErdPanel tab={tabs.current} />
            {/key}
          {:else}
            <DetailPanel
              onconnect={connectById}
              onedit={(profileId) => {
                const profile = profileOf(profileId);
                if (profile) dialog = { profile };
              }}
              ondelete={(profileId) => (confirmDelete = profileOf(profileId))}
              ongroup={(name) => (groupDialog = name)}
              onquery={openQuery}
              ondata={openData}
              onerd={openErd}
            />
          {/if}
        </div>
      </main>
    </div>

    <!--
      Barra de estado: dónde está parado uno. En una aplicación con árbol, pestañas y diálogos, el
      nombre de una tabla no dice contra qué servidor se está trabajando, y esa es justo la
      pregunta que conviene poder contestar sin hacer clic.
    -->
    <footer
      class="panel divider-t flex h-6 shrink-0 items-center gap-2 px-3 text-[11px] muted"
    >
      {#if context}
        <span class="dot {context.connected ? 'dot-on' : 'dot-off'}"></span>
        <span class="font-medium text-zinc-600 dark:text-zinc-300">{context.server}</span>
        {#if context.path}
          <span class="truncate">{context.path}</span>
        {/if}
        {#if context.version}
          <span class="ml-auto shrink-0">{context.version}</span>
        {/if}
      {:else}
        <span>Elegí un objeto del árbol para ver su detalle.</span>
      {/if}
    </footer>
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

{#if groupDialog}
  <GroupDialog name={groupDialog} onclose={() => (groupDialog = null)} />
{/if}

{#if newGroupDialog}
  <NewGroupDialog onclose={() => (newGroupDialog = false)} />
{/if}

{#if prompt}
  <Modal
    title="Contraseña de {prompt.profile.name}"
    subtitle="{prompt.profile.user}@{prompt.profile.host}:{prompt.profile.port}"
    size="sm"
    onclose={() => (prompt = null)}
  >
    <Alert tone="bad" box>{prompt.message}</Alert>

    <label class="mt-3 flex flex-col gap-1">
      <span class="label">Contraseña</span>
      <input
        class="field"
        type="password"
        autocomplete="off"
        data-autofocus
        bind:value={prompt.password}
        onkeydown={(event) => {
          if (event.key === "Enter" && prompt) connect(prompt.profile, prompt.password);
        }}
      />
    </label>

    {#snippet footer()}
      <button class="btn ml-auto" onclick={() => (prompt = null)}>Cancelar</button>
      <button
        class="btn btn-primary"
        onclick={() => prompt && connect(prompt.profile, prompt.password)}
      >
        Conectar
      </button>
    {/snippet}
  </Modal>
{/if}

{#if hostKey}
  <Confirm
    title="Clave del bastión SSH sin verificar"
    message="El host {hostKey.host} {hostKey.changed
      ? 'presentó una clave distinta de la registrada, lo que podría indicar un intermediario.'
      : 'no está en tu archivo known_hosts.'} Huella SHA256: {hostKey.fingerprint}. ¿Confiar en esta clave y recordarla?"
    confirmLabel={hostKey.changed ? "Confiar de todos modos" : "Confiar y conectar"}
    danger={hostKey.changed}
    onconfirm={() => {
      const pending = hostKey;
      hostKey = null;
      if (pending) connect(pending.profile, pending.password, true);
    }}
    onclose={() => (hostKey = null)}
  />
{/if}

{#if confirmDelete}
  <Confirm
    title="Eliminar «{confirmDelete.name}»"
    message="Se borra el servidor de la lista y su contraseña guardada. No se toca nada en la base de datos."
    confirmLabel="Eliminar"
    onconfirm={() => confirmDelete && remove(confirmDelete)}
    onclose={() => (confirmDelete = null)}
  />
{/if}
