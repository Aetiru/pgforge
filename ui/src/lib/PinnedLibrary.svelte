<script lang="ts">
  /**
   * La biblioteca fija al pie del panel lateral: marcadores y scripts, siempre a la vista con
   * cualquier panel que elija el riel (`view.pane`), porque se monta como hermano de ese bloque en
   * `App.svelte` y no adentro de él.
   *
   * El conmutador es el mismo `.seg` de `LibraryPanel.svelte`; el plegado sigue el precedente de
   * `editorSplit.hidden` (`editor.svelte.ts`): la cabecera queda siempre a la vista con su botón,
   * porque una sección que se esconde entera sin dejar de dónde agarrarla no se vuelve a abrir.
   */
  import Alert from "./Alert.svelte";
  import BookmarksList from "./BookmarksList.svelte";
  import Icon from "./Icon.svelte";
  import ScriptsTree from "./ScriptsTree.svelte";
  import { bookmarks, revealBookmark } from "./bookmarks.svelte";
  import { dock, LIBRARY_DEFAULT } from "./dock.svelte";
  import { explorer } from "./explorer.svelte";
  import { describeError, type Bookmark } from "./ipc";
  import { openScript } from "./query.svelte";
  import { view } from "./view.svelte";

  let { onconnect }: { onconnect: (profileId: string) => void } = $props();

  let mode = $state<"bookmarks" | "scripts">("bookmarks");
  let error = $state<string | null>(null);
  let content = $state<HTMLDivElement | null>(null);

  /**
   * Mismo patrón que `startInspectorResize` de `App.svelte`: el alto nuevo se mide contra el borde
   * de abajo del contenido —capturado una sola vez al empezar a arrastrar, no en cada movimiento—,
   * así que el resultado no depende de la posición del cursor sobre la ventana sino de cuánto se
   * movió respecto de ese borde fijo.
   */
  function startResize(event: MouseEvent) {
    event.preventDefault();
    const bottom = content?.getBoundingClientRect().bottom;
    const move = (moved: MouseEvent) => {
      dock.setLibraryHeight((bottom ?? window.innerHeight) - moved.clientY);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("cursor-row-resize");
    };
    document.body.classList.add("cursor-row-resize");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }

  /**
   * Clic en un marcador: revela el objeto en el árbol (`revealBookmark`, en `bookmarks.svelte.ts`).
   *
   * Con el servidor sin conectar se ofrece conectar en vez de fallar —mismo criterio que ya tiene el
   * árbol con un servidor caído—: `onconnect` es el mismo `connectById` que usan `TreePanel` y
   * `DetailPanel`, así que el flujo de contraseña y clave de host SSH es uno solo en toda la
   * aplicación.
   */
  async function openBookmark(bookmark: Bookmark) {
    error = null;
    if (!explorer.isConnected(bookmark.profileId)) {
      onconnect(bookmark.profileId);
      return;
    }
    // El árbol tiene que estar a la vista para que revelar el objeto signifique algo.
    view.show("explorer");
    try {
      const row = await revealBookmark(bookmark);
      if (!row) {
        error = `No se encontró «${bookmark.schema}.${bookmark.name}»: puede que ya no exista o haya cambiado de nombre.`;
      }
    } catch (failure) {
      error = describeError(failure);
    }
  }

  /** Mismo criterio que `openBookmark`: con el servidor sin conectar se ofrece conectar en vez de
   * abrir una pestaña que va a fallar contra la sesión — `openQuery` traga ese error adentro del
   * registro de la pestaña y la deja en la vista de mensajes, con el SQL cargado pero sin mostrarlo. */
  function openFile(path: string, profileId: string, database: string) {
    error = null;
    if (!explorer.isConnected(profileId)) {
      onconnect(profileId);
      return;
    }
    openScript(path, profileId, database).catch((failure) => (error = describeError(failure)));
  }
</script>

<div class="panel flex shrink-0 flex-col">
  <!--
    El separador mide un píxel a la vista pero atrapa el mouse en siete, igual que los de `App.svelte`.
    Se queda montado con la sección colapsada —agarrar el borde de la cabecera para reabrirla es
    parte de lo mismo— pero no responde al arrastre ahí: no hay nada que medir contra un contenido
    que no se está dibujando.
  -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="group relative h-px shrink-0 bg-zinc-200 dark:bg-zinc-700"
    onmousedown={dock.libraryOpen ? startResize : undefined}
    ondblclick={() => dock.setLibraryHeight(LIBRARY_DEFAULT)}
    title={dock.libraryOpen
      ? "Arrastrá para cambiar el alto de la biblioteca; doble clic para restablecerlo"
      : undefined}
  >
    {#if dock.libraryOpen}
      <div
        class="absolute inset-x-0 -top-[3px] h-[7px] cursor-row-resize transition-colors
               group-hover:bg-blue-500/40"
      ></div>
    {/if}
  </div>

  <div class="divider-b flex items-center gap-1.5 px-2 py-1.5">
    <div class="seg" role="tablist">
      <button
        class="seg-item"
        role="tab"
        aria-selected={mode === "bookmarks"}
        onclick={() => (mode = "bookmarks")}
      >
        <Icon name="star" size={12} />
        Marcadores
        {#if bookmarks.entries.length > 0}
          <span class="seg-count">{bookmarks.entries.length}</span>
        {/if}
      </button>
      <button
        class="seg-item"
        role="tab"
        aria-selected={mode === "scripts"}
        onclick={() => (mode = "scripts")}
      >
        <Icon name="sql" size={12} />
        Scripts
      </button>
    </div>

    <button
      class="btn btn-ghost btn-icon btn-toggle ml-auto"
      aria-label={dock.libraryOpen ? "Colapsar la biblioteca" : "Mostrar la biblioteca"}
      aria-pressed={dock.libraryOpen}
      title={dock.libraryOpen ? "Colapsar la biblioteca" : "Mostrar la biblioteca"}
      onclick={() => dock.toggleLibrary()}
    >
      <Icon name="panel-bottom" size={12} />
    </button>
  </div>

  {#if dock.libraryOpen}
    <div bind:this={content} class="min-h-0 overflow-hidden" style="height: {dock.libraryHeight}px">
      {#if error}
        <Alert tone="bad" onclose={() => (error = null)}>{error}</Alert>
      {/if}

      {#if mode === "bookmarks"}
        <BookmarksList onopen={openBookmark} />
      {:else}
        <ScriptsTree onopen={openFile} />
      {/if}
    </div>
  {/if}
</div>
