<script lang="ts">
  import Alert from "./Alert.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import {
    describeError,
    listGroups,
    workspaceCreate,
    workspaceList,
    workspaceOpen,
    workspaceRename,
    type Workspace,
  } from "./ipc";

  /**
   * Ventana nueva de workspace: una ventana de escritorio aparte, acotada a una carpeta de
   * servidores (o a todo el árbol). Junta las dos formas de llegar a una —crearla o reabrir una que
   * ya existe— en el mismo diálogo, en vez de un menú aparte: son decenas de workspaces cuanto mucho,
   * no una lista que necesite su propio componente.
   */
  let {
    onclose,
    ondelete,
    reload = 0,
  }: {
    onclose: () => void;
    /**
     * Borrar es destructivo —aunque no toque perfiles ni servidores, borra la referencia guardada—
     * y por eso no se confirma acá adentro: el diálogo de confirmación no se anida dentro de otro
     * (ver `Confirm.svelte` y cómo lo usa `App.svelte` para borrar un perfil). Este diálogo solo
     * avisa qué workspace se pidió borrar.
     */
    ondelete: (workspace: Workspace) => void;
    /** Cambia cuando `App.svelte` borró un workspace desde el `Confirm` que levantó, para releer la lista. */
    reload?: number;
  } = $props();

  let name = $state("");
  /** Cadena vacía es «todo el árbol»: `workspaceCreate` la manda como `undefined`. */
  let rootGroup = $state("");
  let groups = $state<string[]>([]);
  let workspaces = $state<Workspace[]>([]);
  let loading = $state(true);
  let creating = $state(false);
  /** El workspace que se está abriendo desde la lista, para deshabilitar solo su botón. */
  let openingId = $state<string | null>(null);
  let error = $state<string | null>(null);

  /** El workspace que se está renombrando en la lista, y el texto a medio escribir. */
  let renamingId = $state<string | null>(null);
  let renameValue = $state("");
  let renaming = $state(false);
  let renameInput = $state<HTMLInputElement | null>(null);

  // `data-autofocus` de `Modal` solo se aplica al abrir el diálogo entero: el `<input>` de acá nace
  // más tarde, al convertir la fila en edición, así que el foco se lleva a mano.
  $effect(() => {
    if (renamingId) renameInput?.select();
  });

  function load() {
    Promise.all([listGroups(), workspaceList()])
      .then(([g, w]) => {
        groups = g;
        workspaces = w;
      })
      .catch((e) => (error = describeError(e)))
      .finally(() => (loading = false));
  }

  $effect(() => {
    reload;
    load();
  });

  function startRename(workspace: Workspace) {
    renamingId = workspace.id;
    renameValue = workspace.name;
  }

  function cancelRename() {
    renamingId = null;
  }

  /** Guarda el nombre nuevo con Enter o al perder el foco; sin cambios o vacío, no llama a nada. */
  async function saveRename() {
    const id = renamingId;
    if (!id) return;
    const trimmed = renameValue.trim();
    const current = workspaces.find((w) => w.id === id);
    if (!trimmed || !current || trimmed === current.name) {
      renamingId = null;
      return;
    }
    renaming = true;
    try {
      await workspaceRename(id, trimmed);
      workspaces = workspaces.map((w) => (w.id === id ? { ...w, name: trimmed } : w));
      renamingId = null;
    } catch (e) {
      error = describeError(e);
    } finally {
      renaming = false;
    }
  }

  async function createAndOpen() {
    const trimmed = name.trim();
    if (!trimmed) {
      error = "Poné un nombre para el workspace.";
      return;
    }
    creating = true;
    error = null;
    try {
      const created = await workspaceCreate(trimmed, rootGroup || undefined);
      await workspaceOpen(created.id);
      onclose();
    } catch (e) {
      error = describeError(e);
    } finally {
      creating = false;
    }
  }

  async function openExisting(workspace: Workspace) {
    openingId = workspace.id;
    error = null;
    try {
      await workspaceOpen(workspace.id);
      onclose();
    } catch (e) {
      error = describeError(e);
    } finally {
      openingId = null;
    }
  }
</script>

<Modal
  title="Ventana de workspace"
  subtitle="Una ventana aparte, acotada a una carpeta de servidores"
  size="md"
  busy={creating}
  {onclose}
>
  <div class="flex flex-col gap-1">
    <span class="label">Nueva ventana</span>
    <input
      class="field"
      placeholder="Nombre del workspace"
      data-autofocus
      bind:value={name}
      onkeydown={(event) => {
        if (event.key === "Enter") createAndOpen();
      }}
    />

    <label class="mt-1 flex flex-col gap-1">
      <span class="label">Carpeta raíz</span>
      <select class="field" bind:value={rootGroup}>
        <option value="">Todo el árbol</option>
        {#each groups as group (group)}
          <option value={group}>{group}</option>
        {/each}
      </select>
    </label>

    <p class="mt-1 text-xs muted">
      Abre una ventana nueva que solo muestra esa carpeta y las que cuelgan de ella —o el árbol
      entero, sin elegir ninguna—. No es una copia de los servidores: es la misma lista, mirada
      desde otra ventana.
    </p>

    <button
      class="btn btn-primary mt-2 self-start"
      disabled={creating}
      onclick={createAndOpen}
    >
      {#if creating}<span class="spinner"></span>{/if}
      <Icon name="window" size={12} />
      Crear y abrir
    </button>
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-3">{error}</Alert>
  {/if}

  <div class="divider-t my-3"></div>

  <div class="flex flex-col gap-1">
    <span class="label">Ventanas guardadas</span>
    {#if loading}
      <p class="flex items-center gap-2 text-sm muted"><span class="spinner"></span> Cargando…</p>
    {:else if workspaces.length === 0}
      <p class="text-xs muted">Todavía no se creó ningún workspace.</p>
    {:else}
      <ul class="flex flex-col gap-0.5">
        {#each workspaces as workspace (workspace.id)}
          <li
            class="group flex items-center gap-2 rounded-md px-2 py-1.5
                   hover:bg-zinc-100 dark:hover:bg-zinc-700/70"
          >
            <Icon name="window" size={13} class="muted shrink-0" />
            {#if renamingId === workspace.id}
              <input
                bind:this={renameInput}
                class="field min-w-0 flex-1 py-0.5 text-sm"
                bind:value={renameValue}
                disabled={renaming}
                onblur={saveRename}
                onkeydown={(event) => {
                  if (event.key === "Enter") saveRename();
                  else if (event.key === "Escape") cancelRename();
                }}
              />
            {:else}
              <span class="min-w-0 flex-1 truncate text-sm">{workspace.name}</span>
            {/if}
            <span
              class="max-w-32 shrink-0 truncate text-xs muted"
              title={workspace.rootGroup ?? "todo el árbol"}
            >
              {workspace.rootGroup ?? "todo el árbol"}
            </span>
            <div class="row-actions shrink-0">
              <button
                class="btn btn-icon btn-ghost btn-sm"
                title="Renombrar «{workspace.name}»"
                aria-label="Renombrar"
                onclick={() => startRename(workspace)}
              >
                <Icon name="edit" size={11} />
              </button>
              <button
                class="btn btn-icon btn-sm btn-danger-ghost"
                title="Borrar «{workspace.name}»"
                aria-label="Borrar"
                onclick={() => ondelete(workspace)}
              >
                <Icon name="trash" size={11} />
              </button>
            </div>
            <button
              class="btn btn-ghost btn-sm shrink-0"
              disabled={openingId === workspace.id}
              onclick={() => openExisting(workspace)}
            >
              {#if openingId === workspace.id}<span class="spinner"></span>{/if}
              Abrir
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
  {/snippet}
</Modal>
