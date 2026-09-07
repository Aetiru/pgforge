<script lang="ts">
  /**
   * El árbol de scripts: una carpeta por conexión, con sus subcarpetas y sus `.sql`.
   *
   * `ScriptFolder` no trae el `profileId` de la conexión —el núcleo solo conoce el disco, no
   * `connections.json`—, así que acá se reconstruye con `scriptFolderName` (`script-folder.ts`),
   * espejo de `pgforge_core::scripts::folder_name` hecho **solo** para esta pregunta de solo
   * lectura; no se usa para armar ninguna ruta que se mande a escribir o a borrar, esas siempre
   * llegan enteras del backend (`ScriptFolder.path` / `ScriptEntry.path`).
   */
  import { open } from "@tauri-apps/plugin-dialog";
  import Alert from "./Alert.svelte";
  import Confirm from "./Confirm.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import ScriptFolderRow, { type MenuTarget } from "./ScriptFolderRow.svelte";
  import { explorer } from "./explorer.svelte";
  import {
    describeError,
    scriptCreateFolder,
    scriptDelete,
    scriptRename,
    scriptsImportFolder,
    type ConnectionProfile,
    type ScriptEntry,
    type ScriptFolder,
  } from "./ipc";
  import { childPath, siblingPath } from "./script-name";
  import { scriptFolderName } from "./script-folder";
  import { scripts } from "./scripts.svelte";

  let {
    onopen,
  }: {
    /** Contra qué servidor y base abrir el archivo lo resuelve este componente, no quien lo monta. */
    onopen: (path: string, profileId: string, database: string) => void;
  } = $props();

  /** La carpeta de primer nivel —una por conexión— que contiene esta ruta. */
  function connectionFolderOf(path: string): ScriptFolder | null {
    return (
      scripts.tree.find(
        (folder) =>
          path === folder.path ||
          path.startsWith(`${folder.path}/`) ||
          path.startsWith(`${folder.path}\\`),
      ) ?? null
    );
  }

  /** El perfil dueño de una carpeta de primer nivel, por su nombre ya saneado. */
  function profileOfConnectionFolder(folder: ScriptFolder | null): ConnectionProfile | null {
    if (!folder) return null;
    return (
      explorer.profiles.find((profile) => scriptFolderName(profile.name) === folder.name) ?? null
    );
  }

  function openFile(file: ScriptEntry) {
    const profile = profileOfConnectionFolder(connectionFolderOf(file.path));
    if (!profile) {
      error = `No se pudo determinar a qué conexión pertenece «${file.name}»: puede que el servidor se haya renombrado o borrado.`;
      return;
    }
    onopen(file.path, profile.id, profile.database);
  }

  let error = $state<string | null>(null);

  // ---------------------------------------------------------------------------
  // Menú del clic derecho
  // ---------------------------------------------------------------------------

  let menu = $state<{ x: number; y: number; target: MenuTarget } | null>(null);

  function openMenu(event: MouseEvent, target: MenuTarget) {
    event.preventDefault();
    menu = { x: event.clientX, y: event.clientY, target };
  }

  function startRename() {
    const target = menu?.target;
    menu = null;
    if (!target) return;
    if (target.kind === "folder") {
      if (target.isConnection) return; // Ver comentario del botón: no se ofrece en el menú.
      renaming = { path: target.folder.path, label: target.folder.name, kind: "folder", value: target.folder.name };
    } else {
      renaming = { path: target.file.path, label: target.file.name, kind: "file", value: target.file.name };
    }
  }

  /**
   * Solo archivos: `script_delete` borra con `std::fs::remove_file` (`pgforge_core::scripts::delete`),
   * que falla contra un directorio. Borrar una carpeta entera no tiene comando propio del lado del
   * núcleo —hueco real, no algo para resolver acá—, así que el menú no ofrece la acción sobre una
   * carpeta en vez de mostrar un botón que siempre termina en error.
   */
  function startDelete() {
    const target = menu?.target;
    menu = null;
    if (target?.kind === "file") {
      deleting = { path: target.file.path, label: target.file.name };
    }
  }

  // Las dos variantes de `MenuTarget` llevan `folder` —la propia, o la que contiene al archivo—, así
  // que no hace falta distinguir el tipo para saber en qué carpeta actuar.
  function startNewFolder() {
    const target = menu?.target;
    menu = null;
    if (!target) return;
    newFolder = { parent: target.folder.path, value: "" };
  }

  function startImport() {
    const target = menu?.target;
    menu = null;
    if (!target) return;
    const profile = profileOfConnectionFolder(connectionFolderOf(target.folder.path));
    void importFolder(profile?.id ?? null);
  }

  // ---------------------------------------------------------------------------
  // Renombrar
  // ---------------------------------------------------------------------------

  let renaming = $state<{
    path: string;
    label: string;
    kind: "file" | "folder";
    value: string;
    busy?: boolean;
    error?: string | null;
  } | null>(null);

  async function confirmRename() {
    const current = renaming;
    if (!current) return;
    const trimmed = current.value.trim();
    if (!trimmed) {
      renaming = null;
      return;
    }
    // La carpeta no lleva `.sql`; el archivo sí, con el mismo saneado que usa `renameQueryTab`
    // (`script-name.ts`) para que renombrar desde acá o desde la pestaña arme la ruta igual.
    const withExtension =
      current.kind === "file" && !trimmed.toLowerCase().endsWith(".sql") ? `${trimmed}.sql` : trimmed;
    const newPath = siblingPath(current.path, withExtension);

    current.busy = true;
    current.error = null;
    try {
      await scriptRename(current.path, newPath);
      renaming = null;
      await scripts.load();
    } catch (failure) {
      current.error = describeError(failure);
      current.busy = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Borrar
  // ---------------------------------------------------------------------------

  let deleting = $state<{ path: string; label: string; busy?: boolean; error?: string | null } | null>(
    null,
  );

  async function confirmDelete() {
    const current = deleting;
    if (!current) return;
    current.busy = true;
    current.error = null;
    try {
      await scriptDelete(current.path);
      deleting = null;
      await scripts.load();
    } catch (failure) {
      current.error = describeError(failure);
      current.busy = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Carpeta nueva
  // ---------------------------------------------------------------------------

  let newFolder = $state<{ parent: string; value: string; busy?: boolean; error?: string | null } | null>(
    null,
  );

  async function confirmNewFolder() {
    const current = newFolder;
    if (!current) return;
    const trimmed = current.value.trim();
    if (!trimmed) {
      newFolder = null;
      return;
    }
    current.busy = true;
    current.error = null;
    try {
      await scriptCreateFolder(childPath(current.parent, trimmed));
      newFolder = null;
      await scripts.load();
    } catch (failure) {
      current.error = describeError(failure);
      current.busy = false;
    }
  }

  // ---------------------------------------------------------------------------
  // Importar carpeta
  // ---------------------------------------------------------------------------

  /**
   * `profileId` viene resuelto cuando el menú se abrió sobre la carpeta de una conexión (o algo
   * adentro); si no, se ofrece un selector entre los servidores conectados —no hace falta que
   * `scripts_import_folder` pida contraseña, así que alcanza con estar conectado y no con abrir
   * ninguna sesión nueva.
   */
  let importing = $state<{
    src: string;
    profileId: string;
    busy?: boolean;
    error?: string | null;
  } | null>(null);
  let importReport = $state<{ imported: number; skipped: string[] } | null>(null);

  const connectedProfiles = $derived(
    explorer.servers.filter((row) => row.connected).map((row) => row.profileId),
  );

  async function importFolder(defaultProfileId: string | null) {
    error = null;
    const chosen = await open({ directory: true, title: "Carpeta con scripts .sql" }).catch(
      () => null,
    );
    if (typeof chosen !== "string") return;

    const profileId = defaultProfileId ?? connectedProfiles[0];
    if (!profileId) {
      error = "Conectá algún servidor primero: el script importado tiene que quedar en la carpeta de una conexión.";
      return;
    }
    importing = { src: chosen, profileId };
  }

  async function confirmImport() {
    const current = importing;
    if (!current) return;
    current.busy = true;
    current.error = null;
    try {
      importReport = await scriptsImportFolder(current.profileId, current.src);
      importing = null;
      await scripts.load();
    } catch (failure) {
      current.error = describeError(failure);
      current.busy = false;
    }
  }

  function profileName(profileId: string): string {
    return explorer.profiles.find((profile) => profile.id === profileId)?.name ?? profileId;
  }
</script>

<svelte:window onclick={() => (menu = null)} />

<div class="flex h-full flex-col">
  <div class="divider-b flex items-center gap-1 px-2 py-1.5">
    <span class="text-xs font-medium muted">Scripts de cada conexión</span>
    <button
      class="btn btn-ghost btn-icon ml-auto"
      title="Volver a leer el árbol de scripts"
      aria-label="Volver a leer"
      onclick={() => scripts.load()}
    >
      <Icon name="refresh" size={11} />
    </button>
    <button
      class="btn btn-ghost btn-icon"
      title="Importar una carpeta con archivos .sql"
      aria-label="Importar carpeta"
      onclick={() => importFolder(null)}
    >
      <Icon name="upload" size={11} />
    </button>
  </div>

  {#if error}
    <Alert tone="bad" onclose={() => (error = null)}>{error}</Alert>
  {/if}
  {#if scripts.error}
    <Alert tone="bad" onclose={() => (scripts.error = null)}>{scripts.error}</Alert>
  {/if}

  {#if !scripts.loading && scripts.tree.length === 0}
    <Empty
      icon="folder"
      title="Sin scripts"
      hint="Cada pestaña de consulta se guarda sola en una carpeta por conexión. También podés importar una carpeta con .sql ya escritos."
    />
  {:else}
    <div class="min-h-0 flex-1 overflow-auto py-1">
      {#each scripts.tree as folder, index (folder.path)}
        <ScriptFolderRow
          {folder}
          depth={0}
          isConnection
          separated={index > 0}
          onopenfile={openFile}
          onmenu={openMenu}
        />
      {/each}
    </div>
  {/if}
</div>

{#if menu}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="card fixed z-40 min-w-48 p-1 text-sm shadow-lg"
    style="left: {Math.min(menu.x, window.innerWidth - 220)}px; top: {Math.min(menu.y, window.innerHeight - 200)}px"
    role="menu"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
  >
    {#if menu.target.kind === "file"}
      <button
        class="row-menu"
        onclick={() => {
          const file = menu?.target.kind === "file" ? menu.target.file : null;
          menu = null;
          if (file) openFile(file);
        }}
      >
        <span class="flex items-center gap-2"><Icon name="sql" size={13} /> Abrir</span>
      </button>
    {/if}

    {#if menu.target.kind === "folder"}
      <button class="row-menu" onclick={startNewFolder}>
        <span class="flex items-center gap-2"><Icon name="plus" size={13} /> Nueva subcarpeta</span>
      </button>
      <button class="row-menu" onclick={startImport}>
        <span class="flex items-center gap-2">
          <Icon name="upload" size={13} /> Importar carpeta…
        </span>
      </button>
    {/if}

    {#if !(menu.target.kind === "folder" && menu.target.isConnection)}
      <div class="divider-t my-1"></div>
      <button class="row-menu" onclick={startRename}>
        <span class="flex items-center gap-2"><Icon name="edit" size={13} /> Renombrar</span>
      </button>
      <!-- Solo archivos: ver el comentario de `startDelete` sobre por qué una carpeta no se puede
           borrar desde acá todavía. -->
      {#if menu.target.kind === "file"}
        <button class="row-menu text-rose-600 dark:text-rose-400" onclick={startDelete}>
          <span class="flex items-center gap-2"><Icon name="trash" size={13} /> Borrar</span>
        </button>
      {/if}
    {/if}
  </div>
{/if}

{#if renaming}
  <Confirm
    title="Renombrar «{renaming.label}»"
    message="Nuevo nombre:"
    confirmLabel="Renombrar"
    danger={false}
    busy={renaming.busy}
    error={renaming.error}
    onconfirm={confirmRename}
    onclose={() => (renaming = null)}
  >
    <input
      class="field w-full"
      data-autofocus
      bind:value={renaming.value}
      onkeydown={(event) => event.key === "Enter" && confirmRename()}
    />
  </Confirm>
{/if}

{#if deleting}
  <Confirm
    title="Borrar «{deleting.label}»"
    message="Se borra del disco. No se puede deshacer."
    confirmLabel="Borrar"
    busy={deleting.busy}
    error={deleting.error}
    onconfirm={confirmDelete}
    onclose={() => (deleting = null)}
  />
{/if}

{#if newFolder}
  <Confirm
    title="Nueva subcarpeta"
    message="Nombre de la carpeta:"
    confirmLabel="Crear"
    danger={false}
    busy={newFolder.busy}
    error={newFolder.error}
    onconfirm={confirmNewFolder}
    onclose={() => (newFolder = null)}
  >
    <input
      class="field w-full"
      data-autofocus
      bind:value={newFolder.value}
      onkeydown={(event) => event.key === "Enter" && confirmNewFolder()}
    />
  </Confirm>
{/if}

{#if importing}
  <Confirm
    title="Importar carpeta"
    message="Se copian los .sql de «{importing.src}» —con sus subcarpetas— a la biblioteca de «{profileName(importing.profileId)}». Un nombre repetido no se pisa: entra con « (2)»."
    confirmLabel="Importar"
    danger={false}
    busy={importing.busy}
    error={importing.error}
    onconfirm={confirmImport}
    onclose={() => (importing = null)}
  >
    {#if connectedProfiles.length > 1}
      <label class="flex flex-col gap-1">
        <span class="label">Conexión de destino</span>
        <select class="field" bind:value={importing.profileId}>
          {#each connectedProfiles as profileId (profileId)}
            <option value={profileId}>{profileName(profileId)}</option>
          {/each}
        </select>
      </label>
    {/if}
  </Confirm>
{/if}

{#if importReport}
  <Confirm
    title="Importación terminada"
    message="Entraron {importReport.imported} {importReport.imported === 1 ? 'archivo' : 'archivos'}.{importReport.skipped.length > 0 ? ` ${importReport.skipped.length} no se pudieron copiar.` : ''}"
    confirmLabel="Cerrar"
    danger={false}
    onconfirm={() => (importReport = null)}
    onclose={() => (importReport = null)}
  >
    {#if importReport.skipped.length > 0}
      <ul class="max-h-32 list-disc overflow-auto pl-4 text-xs text-rose-600 dark:text-rose-400">
        {#each importReport.skipped as reason (reason)}
          <li>{reason}</li>
        {/each}
      </ul>
    {/if}
  </Confirm>
{/if}
