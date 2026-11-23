<script lang="ts">
  import Icon, { type IconName } from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { explorer } from "./explorer.svelte";
  import { tasks } from "./tasks.svelte";
  import { theme } from "./theme.svelte";
  import { view, type SidePane } from "./view.svelte";
  import { dock } from "./dock.svelte";

  let { onpreferences }: { onpreferences: () => void } = $props();

  // Derivados: el texto sigue al idioma vigente en vez de quedar fijo al primer render.
  const PANES: { value: SidePane; label: string; icon: IconName; hint: string }[] = $derived([
    { value: "explorer", label: t("Explorador"), icon: "schema", hint: t("Servidores y catálogo") },
    { value: "library", label: t("Biblioteca"), icon: "star", hint: t("Historial y consultas guardadas") },
    { value: "processes", label: t("Procesos"), icon: "clock", hint: t("Lo que corre en segundo plano") },
  ]);

  const THEME_LABEL = $derived({
    system: t("Tema: el del sistema"),
    light: t("Tema: claro"),
    dark: t("Tema: oscuro"),
    paper: t("Tema: papel"),
  } as const);

  const THEME_ICON = { system: "auto", light: "sun", dark: "moon", paper: "edit" } as const;
</script>

<!--
  El riel reemplazó a la barra de vistas de arriba. No es el mismo control mudado de lugar: aquella
  cambiaba **la pantalla entera** —el monitoreo se llevaba puestos el árbol y las pestañas—, y este
  cambia solo qué muestra el panel de al lado. Lo que era una vista con un servidor adentro
  (monitoreo, configuración) ahora es una pestaña, que es donde vive todo lo que tiene servidor.
-->
<nav
  class="panel divider-r flex w-12 shrink-0 flex-col items-center gap-1 py-2"
  aria-label={t("Paneles")}
>
  <span
    class="mb-1 grid size-7 place-items-center rounded-md bg-blue-600 font-mono text-[11px]
           font-bold text-white shadow-sm shadow-blue-600/30"
    title={explorer.workspaceLabel
      ? t("pgforge · workspace \"{name}\"", { name: explorer.workspaceLabel })
      : "pgforge"}>pg</span
  >

  {#if explorer.workspaceLabel}
    <!-- Solo en la ventana de un workspace. Es una marca y no un rótulo: en 48 píxeles no entra el
         nombre, y el nombre completo está en el `title` del logo y en la barra de estado. -->
    <span
      class="-mt-1 mb-1 grid size-4 place-items-center rounded bg-zinc-200 text-zinc-600
             dark:bg-zinc-700 dark:text-zinc-300"
      title={t("Esta ventana está acotada al workspace \"{name}\"", { name: explorer.workspaceLabel })}
    >
      <Icon name="window" size={10} />
    </span>
  {/if}

  <div class="flex flex-col items-center gap-1" role="tablist" aria-orientation="vertical">
    {#each PANES as pane (pane.value)}
      <button
        class="rail-item"
        role="tab"
        aria-selected={view.pane === pane.value && dock.sidebarOpen}
        aria-label={pane.label}
        title="{pane.label} — {pane.hint}"
        onclick={() => view.toggle(pane.value)}
      >
        <Icon name={pane.icon} size={17} />
        <!-- Cuántos corren, y un punto si algo terminó sin que nadie lo mirara: la lista de
             procesos está pensada justamente para no tener que estar mirándola. -->
        {#if pane.value === "processes" && tasks.running.length > 0}
          <span class="badge-count">{tasks.running.length}</span>
        {:else if pane.value === "processes" && tasks.unseen > 0}
          <span class="badge-count size-2 min-w-0 p-0"></span>
        {/if}
      </button>
    {/each}
  </div>

  <div class="mt-auto flex flex-col items-center gap-1">
    <button
      class="rail-item"
      title={THEME_LABEL[theme.preference]}
      aria-label={THEME_LABEL[theme.preference]}
      onclick={() => theme.cycle()}
    >
      <Icon name={THEME_ICON[theme.preference]} size={16} />
    </button>
    <button
      class="rail-item"
      title={t("Preferencias")}
      aria-label={t("Preferencias")}
      onclick={onpreferences}
    >
      <Icon name="gear" size={16} />
    </button>
  </div>
</nav>
