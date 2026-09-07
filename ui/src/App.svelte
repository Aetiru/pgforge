<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import Alert from "./lib/Alert.svelte";
  import Confirm from "./lib/Confirm.svelte";
  import CompareDialog from "./lib/CompareDialog.svelte";
  import ComparePanel from "./lib/ComparePanel.svelte";
  import ConnectionDialog from "./lib/ConnectionDialog.svelte";
  import GroupDialog from "./lib/GroupDialog.svelte";
  import ImportServersDialog from "./lib/ImportServersDialog.svelte";
  import NewGroupDialog from "./lib/NewGroupDialog.svelte";
  import WorkspaceDialog from "./lib/WorkspaceDialog.svelte";
  import Dashboard from "./lib/Dashboard.svelte";
  import ServerConfig from "./lib/ServerConfig.svelte";
  import DataPanel from "./lib/DataPanel.svelte";
  import DetailPanel from "./lib/DetailPanel.svelte";
  import ErdPanel from "./lib/ErdPanel.svelte";
  import PermissionMatrixPanel from "./lib/PermissionMatrixPanel.svelte";
  import RoleCapabilitiesPanel from "./lib/RoleCapabilitiesPanel.svelte";
  import Empty from "./lib/Empty.svelte";
  import Icon, { type IconName } from "./lib/Icon.svelte";
  import Modal from "./lib/Modal.svelte";
  import Palette from "./lib/Palette.svelte";
  import Preferences from "./lib/Preferences.svelte";
  import Rail from "./lib/Rail.svelte";
  import LibraryPanel from "./lib/LibraryPanel.svelte";
  import ProcessPanel from "./lib/ProcessPanel.svelte";
  import QueryPanel from "./lib/QueryPanel.svelte";
  import Toast from "./lib/Toast.svelte";
  import TreePanel from "./lib/TreePanel.svelte";
  import UpdateDialog from "./lib/UpdateDialog.svelte";
  import { openCompare, CompareTab } from "./lib/compare.svelte";
  import { openConfig, ConfigTab } from "./lib/config.svelte";
  import { dock, INSPECTOR_DEFAULT, SIDEBAR_DEFAULT } from "./lib/dock.svelte";
  import { openMonitor, MonitorTab } from "./lib/monitor.svelte";
  import { openData, DataTab } from "./lib/data.svelte";
  import { openErd, ErdTab } from "./lib/erd.svelte";
  import { openPermissionMatrix, PermissionMatrixTab } from "./lib/permission-matrix.svelte";
  import { openRoleCapabilities, RoleCapabilitiesTab } from "./lib/role-capabilities.svelte";
  import { environmentOf, guard } from "./lib/access.svelte";
  import { explorer } from "./lib/explorer.svelte";
  import { openQuery, openSqlFiles, renameQueryTab, saveQueryTab, QueryTab } from "./lib/query.svelte";
  import { queryTargetOf } from "./lib/tree-actions";
  import { parseQuery, PREFIX_HELP } from "./lib/tree-query";
  import { splitView } from "./lib/split-view.svelte";
  import { tabs, type Tab, type TabKind } from "./lib/tabs.svelte";
  import { tasks } from "./lib/tasks.svelte";
  import { view } from "./lib/view.svelte";
  import { updates } from "./lib/update.svelte";
  import { snippets } from "./lib/snippets.svelte";
  import { bookmarks } from "./lib/bookmarks.svelte";
  import { scripts } from "./lib/scripts.svelte";
  import PinnedLibrary from "./lib/PinnedLibrary.svelte";
  import {
    appInfo,
    deleteProfile,
    describeError,
    formatVersion,
    sshHostKey,
    workspaceDelete,
    type AppInfo,
    type CompareSide,
    type ConnectionProfile,
    type Environment,
    type Workspace,
  } from "./lib/ipc";

  let info = $state<AppInfo | null>(null);
  let dialog = $state<{ profile: ConnectionProfile | null } | null>(null);
  let prompt = $state<{ profile: ConnectionProfile; message: string; password: string } | null>(
    null,
  );
  let confirmDelete = $state<ConnectionProfile | null>(null);
  /** Esquema desde el que se pidió comparar; el otro lado lo elige el diálogo. */
  let compareSource = $state<CompareSide | null>(null);
  /**
   * Pestaña que se quiere cerrar con una transacción abierta. La pregunta va acá y no en
   * `QueryTab.dispose()`, que corre cuando la pestaña ya se cerró y no puede preguntar nada.
   */
  let closingTab = $state<QueryTab | null>(null);

  /**
   * La pestaña que se está renombrando in-place, con doble clic sobre el título o `F2`. Es la única
   * clase de pestaña que se puede: el nombre de una de datos o de un diagrama lo pone el objeto que
   * la abrió, y renombrarla ahí confundiría con «esto pasó a llamarse así en el servidor».
   */
  let renamingTab = $state<string | null>(null);
  let renameValue = $state("");
  let renameInput = $state<HTMLInputElement | null>(null);
  /**
   * `Escape` cierra la edición sacando el campo del DOM, y eso por sí solo dispara su `blur` —que si
   * no se distinguiera confirmaría el renombre a medio cancelar—. Se prende antes de sacarlo y el
   * `onblur` lo consulta para saber que este cierre no cuenta.
   */
  let cancelingRename = false;

  function startRenameTab(tab: Tab) {
    if (!(tab instanceof QueryTab)) return;
    renamingTab = tab.key;
    renameValue = tab.title;
  }

  function cancelRenameTab() {
    cancelingRename = true;
    renamingTab = null;
  }

  // Mismo patrón que `Palette`: el campo recién existe cuando `renamingTab` deja de ser `null`, así
  // que enfocarlo va en un efecto y no en el momento de crearlo.
  $effect(() => {
    if (renamingTab !== null) renameInput?.select();
  });

  async function confirmRenameTab(tab: QueryTab) {
    if (cancelingRename) {
      cancelingRename = false;
      return;
    }
    const value = renameValue;
    renamingTab = null;
    if (value.trim() && value.trim() !== tab.title) await renameQueryTab(tab, value);
  }

  /** Cerrar es inmediato, salvo que se pierdan cambios sin confirmar. */
  function closeTab(tab: Tab) {
    if (tab instanceof QueryTab && tab.txStatus !== "idle") {
      closingTab = tab;
      return;
    }
    tabs.close(tab.key);
  }
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
  /** Abierto mientras se buscan servidores ya configurados en otras herramientas. */
  let importDialog = $state(false);
  /** Abierto mientras se crea o reabre una ventana de workspace. */
  let workspaceDialog = $state(false);
  /** El workspace que se pidió borrar desde `WorkspaceDialog`, a confirmar acá (no anidado). */
  let confirmDeleteWorkspace = $state<Workspace | null>(null);
  /** Cambia cada vez que se borra un workspace, para que `WorkspaceDialog` vuelva a leer su lista. */
  let workspaceReload = $state(0);
  /** La paleta de comandos (Ctrl+K). */
  let paletteOpen = $state(false);
  /** El menú con lo que no se usa todos los días del árbol. */
  let treeMenu = $state(false);
  let banner = $state<string | null>(null);
  let preferences = $state(false);
  const TAB_ICON: Record<TabKind, IconName> = {
    query: "sql",
    data: "table",
    erd: "diagram",
    compare: "compare",
    monitor: "chart",
    config: "sliders",
    permissionMatrix: "role",
    roleCapabilities: "role",
  };

  /** Los mismos colores que las pastillas de entorno, aplicados al ícono de la pestaña. */
  const TAB_TONE: Record<Environment | "none", string> = {
    none: "muted",
    dev: "text-emerald-600 dark:text-emerald-400",
    test: "text-blue-600 dark:text-blue-400",
    prod: "text-rose-600 dark:text-rose-400",
  };

  $effect(() => {
    appInfo().then((value) => (info = value));
    explorer.refreshProfiles().catch((error) => (banner = describeError(error)));
    // Engancharse al registro de procesos es lo primero que hace la ventana, y también lo primero
    // que hace después de recargarse: el primer mensaje trae lo que quedó corriendo del otro lado
    // (ver `tasks.svelte.ts`).
    tasks.watch().catch((error) => (banner = describeError(error)));
    // Sin `await` y sin `catch`: la comprobación de versión no bloquea el arranque y su falla no se
    // le muestra a nadie (ver `update.svelte.ts`).
    updates.check();
    // Igual que la comprobación de versión: sin `await` y sin cartel. Que falten las abreviaturas es
    // peor que tenerlas, pero mucho mejor que un error rojo al abrir la ventana.
    snippets.load();
    // Mismo criterio que las abreviaturas: la sección fija del pie se llena en cuanto responde, sin
    // bloquear ni mostrar un error propio — el suyo, si lo hay, lo muestra `PinnedLibrary`.
    bookmarks.load();
    scripts.load();
  });

  const connectedServers = $derived(explorer.servers.filter((row) => row.connected));

  /**
   * Contra qué servidor abren el monitoreo y la configuración.
   *
   * Antes cada una de esas dos vistas tenía su propio `<select>` en la barra de arriba, y eso eran
   * tres respuestas distintas a «dónde estoy parado» conviviendo en la misma ventana. Ahora la
   * respuesta es una sola y sale de lo mismo que todo el resto: lo elegido en el árbol, la pestaña
   * que se está mirando, o el único servidor conectado si no hay nada elegido.
   */
  const contextServer = $derived.by(() => {
    const selected = explorer.selected;
    if (selected && explorer.isConnected(selected.profileId)) return selected.profileId;
    const current = tabs.current;
    if (current && explorer.isConnected(current.profileId)) return current.profileId;
    return connectedServers[0]?.profileId ?? null;
  });

  function profileOf(profileId: string) {
    return explorer.profiles.find((profile) => profile.id === profileId) ?? null;
  }

  /**
   * Con qué servidor se rotula una pestaña. Sale del perfil y no de un campo de `Tab`: el nombre se
   * puede cambiar desde el diálogo de conexión, y una copia guardada al abrir la pestaña quedaría
   * mostrando el nombre viejo hasta cerrarla.
   */
  function serverName(profileId: string): string {
    return profileOf(profileId)?.name ?? "";
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

  /**
   * Contra qué base buscaría en el servidor: la del objeto elegido, o la del perfil si lo elegido
   * es un servidor conectado. Es la misma regla con la que se abre una consulta (`tree-actions`),
   * y por eso sale de la misma función: dos reglas parecidas para «dónde estoy parado» terminan
   * distinto el día que alguien toca una.
   */
  const searchTarget = $derived(
    explorer.selected
      ? queryTargetOf(explorer.selected, profileOf(explorer.selected.profileId))
      : null,
  );

  function searchServer() {
    const row = explorer.selected;
    if (!row || !searchTarget || parseQuery(explorer.search).text === "") return;
    explorer.searchServer(row.profileId, searchTarget.database, explorer.search);
  }

  /** Limpiar la búsqueda también cierra el resultado del servidor: es la misma caja. */
  function clearSearch() {
    explorer.search = "";
    explorer.clearHits();
  }

  function editProfile(profileId: string) {
    const profile = profileOf(profileId);
    if (profile) dialog = { profile };
  }

  /**
   * Abre el diálogo con una copia del servidor. La copia lleva identificador propio, así que
   * guardarla crea un perfil nuevo en vez de pisar el original.
   *
   * La contraseña no se copia: está en el almacén del sistema operativo bajo el identificador
   * viejo, y ese es justamente el punto de tenerla ahí. La copia la vuelve a pedir al conectar.
   */
  function duplicateProfile(profileId: string) {
    const profile = profileOf(profileId);
    if (!profile) return;
    dialog = {
      profile: {
        ...$state.snapshot(profile),
        id: crypto.randomUUID(),
        name: `${profile.name} (copia)`,
        savePassword: false,
      },
    };
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

  /**
   * Borra la referencia guardada de un workspace. No toca perfiles ni servidores —el borrado real
   * vive en `WorkspaceStore::delete`, del lado de Rust—; acá solo se saca de la lista de «ventanas
   * guardadas». Si una ventana ya abierta seguía apuntando a este workspace, la próxima vez que
   * intente algo que dependa de él (por ejemplo recargarse) se comporta como cualquier resolución
   * fallida — ver `Explorer.loadWorkspace` y `workspaceError`—, no como si nunca hubiera existido.
   */
  async function removeWorkspace(workspace: Workspace) {
    confirmDeleteWorkspace = null;
    try {
      await workspaceDelete(workspace.id);
      workspaceReload += 1;
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
    // El ancho se mide contra el borde izquierdo del panel y no contra el de la ventana: el riel
    // está en el medio, y con `clientX` pelado el panel quedaba 48 píxeles más angosto de lo que
    // decía el cursor.
    const left = (event.currentTarget as HTMLElement).previousElementSibling?.getBoundingClientRect()
      .left;
    const move = (moved: MouseEvent) => {
      dock.setSidebarWidth(moved.clientX - (left ?? 0));
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

  /**
   * El divisor del inspector, del otro lado: se arrastra hacia la izquierda para agrandarlo, así
   * que el ancho se mide desde el borde derecho de la ventana y no desde el izquierdo.
   */
  function startInspectorResize(event: MouseEvent) {
    event.preventDefault();
    const right = (event.currentTarget as HTMLElement).nextElementSibling?.getBoundingClientRect()
      .right;
    const move = (moved: MouseEvent) => {
      dock.setInspectorWidth((right ?? window.innerWidth) - moved.clientX);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("cursor-col-resize");
    };
    document.body.classList.add("cursor-col-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  /**
   * El divisor del panel dividido, mismo patrón que `startResize` de arriba pero calculando una
   * proporción contra el contenedor y no un ancho absoluto: `splitView.ratio` es un porcentaje.
   */
  function startSplitResize(event: MouseEvent) {
    event.preventDefault();
    const container = (event.currentTarget as HTMLElement).parentElement;
    if (!container) return;

    const move = (moved: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      splitView.set((moved.clientX - rect.left) / rect.width);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("cursor-col-resize");
    };
    document.body.classList.add("cursor-col-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  /** Atajos que valen en toda la ventana. Los del editor los maneja CodeMirror, que tiene el foco. */
  /**
   * Contra qué base abriría una consulta lo que está seleccionado en el árbol.
   *
   * Es la misma regla que usan el botón del panel de detalle y el menú del clic derecho: vive en
   * `tree-actions.ts` para que las tres puertas abran lo mismo.
   */
  const queryTarget = $derived.by(() => {
    const row = explorer.selected;
    return queryTargetOf(row, row ? profileOf(row.profileId) : undefined);
  });

  function newQuery() {
    const row = explorer.selected;
    if (!row || !queryTarget) return;
    openQuery(row.profileId, queryTarget.database, queryTarget.title);
  }

  /**
   * Contra qué base se abre un archivo `.sql`. El archivo no lo dice: primero manda la pestaña que
   * se está mirando —abrir un script mientras se trabaja sobre una base es contra esa base— y si
   * no hay ninguna, lo elegido en el árbol.
   */
  /** La pestaña del panel de al lado, si hay una partida. */
  const splitTab = $derived(tabs.all.find((item) => item.key === tabs.split) ?? null);

  const sqlTarget = $derived.by(() => {
    const current = tabs.current;
    if (current instanceof QueryTab) {
      return { profileId: current.profileId, database: current.database };
    }
    const row = explorer.selected;
    if (row && queryTarget) return { profileId: row.profileId, database: queryTarget.database };
    return null;
  });

  async function openSqlDialog() {
    if (!sqlTarget) return;
    try {
      const chosen = await open({
        title: "Abrir una consulta",
        multiple: true,
        filters: [{ name: "SQL", extensions: ["sql"] }],
      });
      if (!chosen) return;
      const paths = Array.isArray(chosen) ? chosen : [chosen];
      await openSqlFiles(paths, sqlTarget.profileId, sqlTarget.database);
    } catch (error) {
      banner = describeError(error);
    }
  }

  function onKeydown(event: KeyboardEvent) {
    // `F2` no lleva `Ctrl`, así que se atiende aparte y antes de la guarda de abajo. Si algo con el
    // foco ya lo atendió —el árbol renombra la carpeta o el servidor elegido, ver `TreePanel`—, ese
    // manejador corrió primero en la fase de burbuja y ya llamó a `preventDefault()`: se respeta y
    // no se abren las dos cosas a la vez.
    if (event.key === "F2" && !event.defaultPrevented) {
      if (tabs.current instanceof QueryTab && renamingTab === null) {
        event.preventDefault();
        startRenameTab(tabs.current);
      }
      return;
    }

    if (!(event.ctrlKey || event.metaKey)) return;

    switch (event.key.toLowerCase()) {
      case "b":
        event.preventDefault();
        dock.toggleSidebar();
        break;
      case "i":
        // El inspector es el antiguo panel de Detalle. Se pliega porque contra una pantalla de
        // portátil, mirar el árbol y escribir SQL a la vez ya se lleva todo el ancho.
        event.preventDefault();
        dock.toggleInspector();
        break;
      case "k":
        // La misma tecla abre y cierra: es lo que uno intenta cuando se abrió sin querer.
        event.preventDefault();
        paletteOpen = !paletteOpen;
        break;
      case "q":
        event.preventDefault();
        newQuery();
        break;
      case "s":
        // Con el editor enfocado lo atiende su propio keymap; esto cubre el resto de la ventana,
        // donde `Ctrl+S` si no lo tomaría el navegador que hay debajo de la ventana de Tauri.
        event.preventDefault();
        if (tabs.current instanceof QueryTab) saveQueryTab(tabs.current, event.shiftKey);
        break;
      case "o":
        event.preventDefault();
        openSqlDialog();
        break;
    }
  }

  /**
   * Abre el dashboard del servidor en contexto. Es una pestaña y no una vista: mirar cómo va el
   * servidor mientras corre una consulta dejó de costar irse de donde uno estaba.
   */
  function showMonitor() {
    if (contextServer) openMonitor(contextServer);
  }

  function showConfig() {
    if (contextServer) openConfig(contextServer);
  }

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

<svelte:window
  onkeydown={onKeydown}
  onclick={() => (treeMenu = false)}
  onblur={() => {
    // La ventana entera perdió el foco —se fue a otra aplicación—, uno de los tres momentos en que
    // se vuelca el script pendiente (los otros dos: cambiar de pestaña activa y cerrarla).
    if (tabs.current instanceof QueryTab) void tabs.current.flushScript();
  }}
/>

<!--
  La carcasa: riel, panel lateral, pestañas e inspector.

  Antes había dos navegaciones superpuestas: una barra de cuatro vistas excluyentes arriba —que al
  cambiar se llevaba puestos el árbol y las pestañas— y adentro de una de ellas la barra de
  pestañas. Eso convertía «mirar el dashboard mientras corre una consulta» en irse, mirar y volver a
  buscar dónde estaba uno. Quedó una sola navegación: lo que tiene un servidor adentro (consulta,
  datos, ERD, comparación, monitoreo, configuración) es una pestaña, y el riel de la izquierda solo
  cambia qué muestra el panel de al lado.
-->
<div class="flex h-full flex-col">
  {#if banner}
    <Alert tone="bad" onclose={() => (banner = null)}>{banner}</Alert>
  {/if}

  <div class="flex min-h-0 flex-1">
    <Rail onpreferences={() => (preferences = true)} />

    {#if dock.sidebarOpen}
      <aside
        class="panel flex min-h-0 flex-col overflow-hidden"
        style="width: {dock.sidebarWidth}px"
      >
        {#if view.pane === "explorer"}
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
                  title="Filtra lo que el árbol ya trajo. Enter busca en el servidor.
  Con prefijo se acota al tipo — {PREFIX_HELP}"
                  bind:value={explorer.search}
                  onkeydown={(event) => {
                    if (event.key === "Escape") clearSearch();
                    // Enter es lo que uno aprieta cuando el filtro no encontró lo que buscaba, así
                    // que ahí es donde tiene que estar la búsqueda que sí alcanza todo el catálogo.
                    if (event.key === "Enter") searchServer();
                  }}
                />
                {#if explorer.search}
                  <button
                    class="btn btn-ghost btn-icon absolute top-1/2 right-0.5 size-6 -translate-y-1/2"
                    aria-label="Limpiar la búsqueda"
                    onclick={clearSearch}
                  >
                    <Icon name="close" size={11} />
                  </button>
                {/if}
              </div>
              <button
                class="btn btn-ghost btn-icon"
                title={searchTarget
                  ? `Buscar «${explorer.search}» en el catálogo de ${searchTarget.database}`
                  : "Elegí una base o un objeto del árbol para buscar en el servidor"}
                aria-label="Buscar en el servidor"
                disabled={!searchTarget || parseQuery(explorer.search).text === "" || explorer.searching}
                onclick={searchServer}
              >
                <Icon name="compass" />
              </button>
              <button
                class="btn btn-ghost btn-icon"
                title="Nuevo servidor"
                aria-label="Nuevo servidor"
                onclick={() => (dialog = { profile: null })}
              >
                <Icon name="plus" />
              </button>

              <!--
                Todo lo demás del árbol vive acá adentro. Eran siete controles repartidos entre la
                barra y el pie del panel, en 300 píxeles de ancho: lo que se usa todos los días es
                buscar y agregar un servidor, y el resto se abre cuando hace falta.
              -->
              <div class="relative">
                <button
                  class="btn btn-ghost btn-icon"
                  title="Más opciones del árbol"
                  aria-label="Más opciones del árbol"
                  aria-expanded={treeMenu}
                  onclick={(event) => {
                    event.stopPropagation();
                    treeMenu = !treeMenu;
                  }}
                >
                  <Icon name="dots" />
                </button>

                {#if treeMenu}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div
                    class="card absolute top-full right-0 z-40 mt-1 min-w-60 p-1 text-sm shadow-lg"
                    role="menu"
                    tabindex="-1"
                    onclick={(event) => event.stopPropagation()}
                  >
                    <button
                      class="row-menu"
                      onclick={() => {
                        treeMenu = false;
                        newGroupDialog = true;
                      }}
                    >
                      <span class="flex items-center gap-2">
                        <Icon name="folder" size={13} /> Nueva carpeta
                      </span>
                    </button>
                    <button
                      class="row-menu"
                      onclick={() => {
                        treeMenu = false;
                        importDialog = true;
                      }}
                    >
                      <span class="flex items-center gap-2">
                        <Icon name="download" size={13} /> Importar servidores…
                      </span>
                    </button>
                    <button
                      class="row-menu"
                      onclick={() => {
                        treeMenu = false;
                        explorer.collapseAll();
                      }}
                    >
                      <span class="flex items-center gap-2">
                        <Icon name="collapse" size={13} /> Contraer todo
                      </span>
                    </button>

                    <div class="divider-t my-1"></div>

                    <!-- Una ventana aparte, acotada a una carpeta: no reemplaza al árbol de acá, lo
                         mira desde otro lado. El diálogo junta crear una nueva y reabrir una guardada,
                         así que un solo botón alcanza. -->
                    <button
                      class="row-menu"
                      onclick={() => {
                        treeMenu = false;
                        workspaceDialog = true;
                      }}
                    >
                      <span class="flex items-center gap-2">
                        <Icon name="window" size={13} /> Ventana de workspace…
                      </span>
                    </button>

                    <div class="divider-t my-1"></div>

                    <!-- Filtra sin releer nada del servidor: es cambiar qué se dibuja, no qué se
                         cargó. -->
                    <label class="check px-2 py-1" title="Esconde los servidores sin conectar">
                      <input type="checkbox" bind:checked={explorer.onlyConnected} />
                      Solo servidores conectados
                    </label>
                    <label class="check px-2 py-1">
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
                {/if}
              </div>
            </div>

            <div class="min-h-0 flex-1 px-1 pb-1">
              <TreePanel
                onconnect={connectById}
                onnew={() => (dialog = { profile: null })}
                onedit={editProfile}
                onduplicate={duplicateProfile}
                ondelete={(profileId) => (confirmDelete = profileOf(profileId))}
                ongroup={(name) => (groupDialog = name)}
                onquery={openQuery}
                ondata={openData}
                onerd={openErd}
                oncompare={(source) => (compareSource = source)}
                onmatrix={openPermissionMatrix}
                onrolecapabilities={openRoleCapabilities}
              />
            </div>
        {:else if view.pane === "library"}
          <LibraryPanel
            profileId={contextServer}
            database={queryTarget?.database ?? null}
            onerror={(message) => (banner = message)}
          />
        {:else}
          <ProcessPanel />
        {/if}

        <!--
          Fija al pie, hermana del bloque de arriba y no adentro de él: se ve con cualquier panel que
          elija el riel —explorador, biblioteca de historial/guardadas, procesos—, y no solo con el
          árbol. `dock.libraryOpen` la pliega sin afectar el resto del panel lateral.
        -->
        <PinnedLibrary onconnect={connectById} />
      </aside>

      <!--
        El separador mide un píxel a la vista pero atrapa el mouse en seis: agarrar una línea de
        un píxel es de las cosas más frustrantes de una interfaz de escritorio.
      -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="group relative w-px shrink-0 bg-zinc-200 dark:bg-zinc-700"
        onmousedown={startResize}
        ondblclick={() => dock.setSidebarWidth(SIDEBAR_DEFAULT)}
        title="Arrastrá para cambiar el ancho; doble clic para restablecerlo"
      >
        <div
          class="absolute inset-y-0 -left-[3px] w-[7px] cursor-col-resize
                 transition-colors group-hover:bg-blue-500/40"
        ></div>
      </div>
    {/if}

    <main class="flex min-w-0 flex-1 flex-col overflow-hidden">
      <!--
        La tira de pestañas y, aparte, las acciones. Van en dos contenedores porque solo el primero
        desplaza: con seis pestañas abiertas, el botón de «nueva consulta» se iba de pantalla justo
        cuando más se lo usaba.
      -->
      <div class="divider-b flex items-stretch">
        <!-- `overflow-y-hidden`: sin eso, el subrayado de la pestaña activa desborda un píxel hacia
             abajo y el navegador dibuja una barra de desplazamiento vertical en toda la tira. -->
        <div
          class="flex min-w-0 flex-1 items-stretch gap-px overflow-x-auto overflow-y-hidden px-1
                 pt-1"
          role="tablist"
        >
          {#each tabs.all as tab (tab.key)}
            <div class="tab-wrap">
              {#if renamingTab === tab.key && tab instanceof QueryTab}
                {@const renaming = tab}
                <!--
                  Afuera de cualquier `<button>`: adentro, la barra espaciadora del título nuevo
                  activaría el botón en vez de escribirse, que es justo el atajo con el que un
                  `<button>` responde a la barra espaciadora.
                -->
                <input
                  bind:this={renameInput}
                  bind:value={renameValue}
                  class="field my-auto h-full max-w-40 rounded-none border-none bg-transparent px-2
                         text-sm shadow-none"
                  onblur={() => confirmRenameTab(renaming)}
                  onkeydown={(event) => {
                    if (event.key === "Enter") event.currentTarget.blur();
                    else if (event.key === "Escape") {
                      event.preventDefault();
                      cancelRenameTab();
                    }
                  }}
                />
              {:else}
                <button
                  class="tab pr-1"
                  role="tab"
                  aria-selected={tabs.active === tab.key}
                  title={`${tab.title} · ${serverName(tab.profileId)}${tab.database ? ` / ${tab.database}` : ""}`}
                  onclick={() => tabs.activate(tab.key)}
                  onauxclick={(event) => {
                    // Botón del medio: cerrar, como en cualquier navegador.
                    if (event.button === 1) closeTab(tab);
                  }}
                >
                  {#if tab instanceof QueryTab && tab.running}
                    <span class="spinner"></span>
                  {:else}
                    <!-- El ícono de la pestaña toma el color del entorno: la pestaña activa tapa el
                         árbol, y sin esto no queda nada en pantalla diciendo que es producción. -->
                    <Icon
                      name={TAB_ICON[tab.kind]}
                      size={12}
                      class={TAB_TONE[environmentOf(tab.profileId) ?? "none"]}
                    />
                  {/if}
                  <!--
                    El servidor va en la pestaña y no solo en el `title`: con cuatro consultas
                    abiertas, «Consulta 1» contra desarrollo y «Consulta 1» contra producción eran la
                    misma pestaña a la vista, y averiguar cuál era cuál pedía pasar el mouse por
                    encima de cada una. Se recorta antes que el nombre de la pestaña porque es el
                    contexto, no lo que se está mirando.
                  -->
                  {#if serverName(tab.profileId)}
                    <span class="max-w-24 shrink truncate text-[11px] muted">
                      {serverName(tab.profileId)}
                    </span>
                    <span class="shrink-0 text-[11px] muted">/</span>
                  {/if}
                  <!--
                    Doble clic sobre el título edita el nombre en el lugar; ver `startRenameTab`.
                    Solo una pestaña de consulta lo admite —la de datos o la de un diagrama llevan el
                    nombre del objeto que las abrió, y renombrarlas ahí confundiría con «esto pasó a
                    llamarse así en el servidor»—.
                  -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span
                    class="truncate"
                    ondblclick={(event) => {
                      if (!(tab instanceof QueryTab)) return;
                      event.stopPropagation();
                      startRenameTab(tab);
                    }}
                  >
                    {tab.title}
                  </span>
                </button>
                <!-- Manda esta pestaña al panel de al lado, o la saca si ya estaba ahí. Deshabilitado
                     sobre la pestaña principal: partirla contra sí misma no significa nada. -->
                <button
                  class="tab-close {tabs.split === tab.key ? 'text-blue-600 dark:text-blue-400' : ''}"
                  aria-label={tabs.split === tab.key ? "Sacar del panel dividido" : "Abrir al lado"}
                  aria-pressed={tabs.split === tab.key}
                  disabled={tab.key === tabs.active}
                  title={tab.key === tabs.active
                    ? "Ya es la pestaña principal"
                    : tabs.split === tab.key
                      ? "Vuelve a mostrar una sola pestaña"
                      : "Abre esta pestaña en un panel al lado, sin dejar de ver la actual"}
                  onclick={() => tabs.toggleSplit(tab.key)}
                >
                  <Icon name="columns" size={10} />
                </button>
                <button
                  class="tab-close"
                  aria-label="Cerrar la pestaña"
                  title="Cerrar la pestaña"
                  onclick={() => closeTab(tab)}
                >
                  <Icon name="close" size={10} />
                </button>
              {/if}
            </div>
          {/each}
        </div>

        <div class="flex shrink-0 items-center gap-0.5 px-1">
          <!-- Abrir una consulta contra lo que ya se está mirando, sin volver al panel de detalle:
               era el camino de todos los días y son dos clics de más cada vez. -->
          <button
            class="btn btn-ghost btn-icon"
            disabled={queryTarget === null}
            aria-label="Nueva consulta"
            title={queryTarget
              ? `Nueva consulta contra ${queryTarget.database} (Ctrl+Q)`
              : "Elegí una base o un objeto de un servidor conectado"}
            onclick={newQuery}
          >
            <Icon name="plus" size={13} />
          </button>

          <!-- Guardar existía desde el principio; abrir, no. Un `.sql` que ya está en disco había
               que abrirlo en otro editor y pegarlo acá. -->
          <button
            class="btn btn-ghost btn-icon"
            disabled={sqlTarget === null}
            aria-label="Abrir un archivo SQL"
            title={sqlTarget
              ? `Abrir un archivo .sql contra ${sqlTarget.database} (Ctrl+O)`
              : "Elegí una base o un objeto de un servidor conectado"}
            onclick={openSqlDialog}
          >
            <Icon name="upload" size={13} />
          </button>

          <span class="mx-1 h-4 w-px bg-zinc-200 dark:bg-zinc-700"></span>

          <!-- Monitoreo y configuración abren contra el servidor en contexto. Eran dos vistas que
               tapaban la pantalla y traían su propio selector de servidor; son dos pestañas más. -->
          <button
            class="btn btn-ghost btn-icon"
            disabled={contextServer === null}
            aria-label="Monitoreo"
            title={contextServer
              ? `Monitoreo de ${serverName(contextServer)}`
              : "Conectá un servidor para ver su monitoreo"}
            onclick={showMonitor}
          >
            <Icon name="chart" size={13} />
          </button>
          <button
            class="btn btn-ghost btn-icon"
            disabled={contextServer === null}
            aria-label="Configuración del servidor"
            title={contextServer
              ? `Configuración de ${serverName(contextServer)}`
              : "Conectá un servidor para ver su configuración"}
            onclick={showConfig}
          >
            <Icon name="sliders" size={13} />
          </button>

          <span class="mx-1 h-4 w-px bg-zinc-200 dark:bg-zinc-700"></span>

          <button
            class="btn btn-ghost btn-icon {dock.inspectorOpen
              ? 'text-blue-600 dark:text-blue-400'
              : ''}"
            aria-label="Inspector"
            aria-pressed={dock.inspectorOpen}
            title={dock.inspectorOpen
              ? "Ocultar el inspector (Ctrl+I)"
              : "Mostrar el inspector (Ctrl+I)"}
            onclick={() => dock.toggleInspector()}
          >
            <Icon name="compass" size={13} />
          </button>
        </div>
      </div>

      {#snippet tabBody(tab: Tab)}
        {#if tab instanceof QueryTab}
          <QueryPanel {tab} />
        {:else if tab instanceof DataTab}
          <DataPanel {tab} />
        {:else if tab instanceof ErdTab}
          <ErdPanel {tab} />
        {:else if tab instanceof CompareTab}
          <ComparePanel {tab} />
        {:else if tab instanceof PermissionMatrixTab}
          <PermissionMatrixPanel {tab} />
        {:else if tab instanceof RoleCapabilitiesTab}
          <RoleCapabilitiesPanel {tab} />
        {:else if tab instanceof MonitorTab}
          <Dashboard profileId={tab.profileId} />
        {:else if tab instanceof ConfigTab}
          <ServerConfig profileId={tab.profileId} />
        {/if}
      {/snippet}

      <div class="flex min-h-0 flex-1 flex-row">
        <div
          class="flex min-h-0 min-w-0 flex-col {tabs.split ? '' : 'flex-1'}"
          style={tabs.split ? `flex: 0 0 ${splitView.ratio * 100}%` : ""}
        >
          {#if tabs.current}
            {#key tabs.current.key}
              {@render tabBody(tabs.current)}
            {/key}
          {:else}
            <!-- Sin pestañas la ventana no está vacía: el objeto elegido se ve en el inspector, y
                 acá van las tres cosas que se abren desde cero. -->
            <Empty
              icon="sql"
              title="No hay ninguna pestaña abierta"
              hint="Elegí algo en el árbol para verlo en el inspector, o abrí una consulta contra la base con la que estés trabajando."
            >
              <button class="btn btn-primary" disabled={queryTarget === null} onclick={newQuery}>
                Nueva consulta
              </button>
              <button class="btn" disabled={contextServer === null} onclick={showMonitor}>
                Monitoreo
              </button>
              <button class="btn" disabled={contextServer === null} onclick={showConfig}>
                Configuración
              </button>
            </Empty>
          {/if}
        </div>

        {#if tabs.split && splitTab}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="group relative h-full w-px shrink-0 bg-zinc-200 dark:bg-zinc-700"
            onmousedown={startSplitResize}
            ondblclick={() => splitView.reset()}
            title="Arrastrá para repartir el espacio; doble clic para repartirlo por la mitad"
          >
            <div
              class="absolute inset-y-0 -left-[3px] w-[7px] cursor-col-resize
                     transition-colors group-hover:bg-blue-500/40"
            ></div>
          </div>

          <div class="flex min-h-0 min-w-0 flex-1 flex-col">
            {#key splitTab.key}
              {@render tabBody(splitTab)}
            {/key}
          </div>
        {/if}
      </div>
    </main>

    {#if dock.inspectorOpen}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="group relative w-px shrink-0 bg-zinc-200 dark:bg-zinc-700"
        onmousedown={startInspectorResize}
        ondblclick={() => dock.setInspectorWidth(INSPECTOR_DEFAULT)}
        title="Arrastrá para cambiar el ancho; doble clic para restablecerlo"
      >
        <div
          class="absolute inset-y-0 -left-[3px] w-[7px] cursor-col-resize
                 transition-colors group-hover:bg-blue-500/40"
        ></div>
      </div>

      <!--
        El inspector es el antiguo panel de «Detalle», que era una pestaña más. Como pestaña, mirar
        las columnas de una tabla mientras se escribía la consulta que las usa costaba cambiar de
        pestaña —y perder de vista el editor— cada vez. Acá sigue al árbol sin tapar nada.
      -->
      <aside
        class="panel flex min-h-0 shrink-0 flex-col overflow-hidden"
        style="width: {dock.inspectorWidth}px"
        aria-label="Inspector"
      >
        <div class="divider-b flex h-8 shrink-0 items-center gap-2 px-2">
          <Icon name="compass" size={12} class="muted" />
          <span class="card-title">Detalle</span>
          <button
            class="btn btn-ghost btn-icon ml-auto"
            aria-label="Ocultar el inspector"
            title="Ocultar el inspector (Ctrl+I)"
            onclick={() => dock.setInspector(false)}
          >
            <Icon name="close" size={11} />
          </button>
        </div>

        <div class="min-h-0 flex-1">
          <DetailPanel
            onconnect={connectById}
            onedit={editProfile}
            ondelete={(profileId) => (confirmDelete = profileOf(profileId))}
            ongroup={(name) => (groupDialog = name)}
            onquery={openQuery}
            ondata={openData}
            onerd={openErd}
            oncompare={(source) => (compareSource = source)}
            onmatrix={openPermissionMatrix}
            onrolecapabilities={openRoleCapabilities}
          />
        </div>
      </aside>
    {/if}
  </div>

  <!--
    Barra de estado: dónde está parado uno. En una aplicación con árbol, pestañas y diálogos, el
    nombre de una tabla no dice contra qué servidor se está trabajando, y esa es justo la pregunta
    que conviene poder contestar sin hacer clic. Al sacar la barra de arriba, también es donde
    quedaron la versión y el aviso de que hay una más nueva: son datos de la ventana, no acciones.
  -->
  <footer class="panel divider-t flex h-6 shrink-0 items-center gap-2 px-3 text-[11px] muted">
    {#if context}
      <span class="dot {context.connected ? 'dot-on' : 'dot-off'}"></span>
      <span class="font-medium text-zinc-600 dark:text-zinc-300">{context.server}</span>
      {#if context.path}
        <span class="truncate">{context.path}</span>
      {/if}
    {:else}
      <span>Elegí un objeto del árbol para ver su detalle.</span>
    {/if}

    <div class="ml-auto flex shrink-0 items-center gap-3">
      {#if connectedServers.length > 0}
        <span class="flex items-center gap-1.5" title="Servidores conectados">
          <span class="dot dot-on"></span>
          {connectedServers.length}
          {connectedServers.length === 1 ? "conectado" : "conectados"}
        </span>
      {/if}

      {#if context?.version}
        <span>{context.version}</span>
      {/if}

      {#if updates.release}
        <!-- Aparece solo cuando hay algo más nuevo publicado. Es una pastilla y no un cartel: la
             versión nueva no interrumpe lo que se estaba haciendo. -->
        <button
          class="tag-ok flex items-center gap-1"
          title="pgforge {updates.release.version} está disponible"
          onclick={() => (updates.showing = true)}
        >
          <Icon name="download" size={11} />
          {updates.release.version}
        </button>
      {/if}

      {#if info}
        <!-- La ruta del registro cuelga de la versión: es lo que se pide junto con ella cuando algo
             falla, y no merece un lugar propio. Hacer clic vuelve a preguntar por una versión nueva
             sin esperar al próximo día. -->
        <button
          class="select-text"
          title="{info.logDir ? `Registro en ${info.logDir}\n` : ''}Buscar una versión nueva"
          onclick={() => updates.check(true)}
        >
          v{info.version}
        </button>
      {/if}
    </div>
  </footer>
</div>

{#if paletteOpen}
  <Palette
    onnewquery={newQuery}
    onopensql={openSqlDialog}
    onnewserver={() => (dialog = { profile: null })}
    onmonitor={showMonitor}
    onconfig={showConfig}
    onconnect={connectById}
    onclose={() => (paletteOpen = false)}
  />
{/if}

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

{#if compareSource}
  <CompareDialog
    source={compareSource}
    onclose={() => (compareSource = null)}
    oncompare={(source, target) => void openCompare(source, target)}
  />
{/if}

{#if updates.showing && info}
  <UpdateDialog current={info.version} onclose={() => (updates.showing = false)} />
{/if}

{#if preferences}
  <Preferences onclose={() => (preferences = false)} />
{/if}

{#if importDialog}
  <ImportServersDialog
    onclose={() => (importDialog = false)}
    onimported={() => (importDialog = false)}
  />
{/if}

{#if newGroupDialog}
  <NewGroupDialog onclose={() => (newGroupDialog = false)} />
{/if}

{#if workspaceDialog}
  <WorkspaceDialog
    onclose={() => (workspaceDialog = false)}
    ondelete={(workspace) => (confirmDeleteWorkspace = workspace)}
    reload={workspaceReload}
  />
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

{#if closingTab}
  <Confirm
    title="Cerrar «{closingTab.title}»"
    message="La pestaña tiene una transacción abierta. Al cerrarla se suelta la conexión y el servidor revierte todo lo que no esté confirmado."
    confirmLabel="Cerrar y revertir"
    onconfirm={() => {
      if (closingTab) tabs.close(closingTab.key);
      closingTab = null;
    }}
    onclose={() => (closingTab = null)}
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

{#if confirmDeleteWorkspace}
  <Confirm
    title="Eliminar «{confirmDeleteWorkspace.name}»"
    message="Se borra la referencia a esta ventana guardada. No se toca ningún servidor ni carpeta de conexiones, y una ventana que la tenga abierta en este momento sigue abierta."
    confirmLabel="Eliminar"
    onconfirm={() => confirmDeleteWorkspace && removeWorkspace(confirmDeleteWorkspace)}
    onclose={() => (confirmDeleteWorkspace = null)}
  />
{/if}

<!--
  Va último a propósito: se dibuja sobre el diálogo de mutación que la pidió, que sigue abierto
  detrás con todo lo que el usuario escribió.
-->
{#if guard.pending}
  <Confirm
    title="Modificar producción"
    message="«{guard.pending.profile.name}» está marcado como servidor de producción. {guard.pending
      .action}"
    confirmLabel="Modificar igual"
    onconfirm={() => guard.answer(true)}
    onclose={() => guard.answer(false)}
  />
{/if}

<Toast />
