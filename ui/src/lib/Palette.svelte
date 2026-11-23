<script lang="ts">
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { EDITOR_ACTIONS } from "./editor/actions";
  import { explorer, type Row } from "./explorer.svelte";
  import { noteHint } from "./notes";
  import { notes } from "./notes.svelte";
  import { rank, type Command, type CommandGroup } from "./palette";
  import { QueryTab } from "./query.svelte";
  import { shortcutStore } from "./shortcuts.svelte";
  import { tabs } from "./tabs.svelte";
  import { theme } from "./theme.svelte";
  import { uiZoom } from "./ui-zoom.svelte";
  import { view, type SidePane } from "./view.svelte";

  /**
   * La paleta de comandos.
   *
   * En una aplicación con árbol, pestañas y treinta diálogos, lo que más tiempo come
   * no es ejecutar la acción sino llegar hasta ella. Acá se escribe lo que se quiere —«monit»,
   * «pedidos», el nombre de un servidor— y se llega en dos teclas.
   *
   * Lo que ofrece sale de lo que la aplicación ya tiene cargado: no consulta nada al servidor. Es
   * deliberado: la paleta tiene que abrirse y responder en el mismo cuadro, y el catálogo entero de
   * una base grande no entra en esa promesa. Para lo que el árbol todavía no trajo está la búsqueda
   * del árbol, que sí pregunta.
   */
  let {
    onnewquery,
    onopensql,
    onnewserver,
    onmonitor,
    onconfig,
    onshortcuts,
    onmigrations,
    onconnect,
    ondisconnectall,
    onclose,
  }: {
    onnewquery: () => void;
    onopensql: () => void;
    onnewserver: () => void;
    /** Abre el dashboard del servidor en contexto, que lo decide `App`. */
    onmonitor: () => void;
    onconfig: () => void;
    onshortcuts: () => void;
    onmigrations: () => void;
    onconnect: (profileId: string) => void;
    /** Pide confirmación y desconecta todo: lo decide `App`, que es quien monta el diálogo. */
    ondisconnectall: () => void;
    onclose: () => void;
  } = $props();

  function themeLabel(preference: "system" | "light" | "dark" | "paper"): string {
    if (preference === "light") return t("claro");
    if (preference === "dark") return t("oscuro");
    if (preference === "paper") return t("papel");
    return t("el del sistema");
  }

  let query = $state("");
  let cursor = $state(0);
  let input = $state<HTMLInputElement | null>(null);

  /**
   * Los paneles laterales. Monitoreo y configuración salieron de esta lista cuando dejaron de ser
   * vistas: son pestañas, y por eso están arriba entre las acciones que abren algo.
   */
  const panes = $derived<{ value: SidePane; label: string }[]>([
    { value: "explorer", label: t("Explorador") },
    { value: "library", label: t("Historial y consultas guardadas") },
    { value: "processes", label: t("Procesos") },
  ]);

  /** Las filas de objeto que el árbol ya trajo, estén a la vista o dentro de una rama plegada. */
  function loadedRows(): Row[] {
    const out: Row[] = [];
    const walk = (rows: Row[]) => {
      for (const row of rows) {
        if (row.kind === "node") out.push(row);
        walk(row.children ?? []);
      }
    };
    walk(explorer.roots);
    return out;
  }

  const commands = $derived.by<Command[]>(() => {
    const out: Command[] = [
      {
        id: "query",
        label: t("Nueva consulta"),
        hint: shortcutStore.label("app.newQuery"),
        group: "acción",
        run: onnewquery,
      },
      { id: "sql", label: t("Abrir un archivo SQL"), hint: shortcutStore.label("app.open"), group: "acción", run: onopensql },
      {
        id: "shortcuts",
        label: t("Atajos de teclado"),
        hint: shortcutStore.label("app.shortcuts"),
        group: "acción",
        run: onshortcuts,
      },
      { id: "server", label: t("Nuevo servidor"), group: "acción", run: onnewserver },
      ...(explorer.connectedCount > 0
        ? [
            {
              id: "disconnect-all",
              label: t("Desconectar todos los servidores ({n})", { n: explorer.connectedCount }),
              group: "acción" as const,
              run: ondisconnectall,
            },
          ]
        : []),
      {
        id: "zoom-in",
        label: t("Agrandar toda la interfaz"),
        hint: uiZoom.percent,
        group: "acción",
        run: () => uiZoom.bigger(),
      },
      {
        id: "zoom-out",
        label: t("Achicar toda la interfaz"),
        hint: uiZoom.percent,
        group: "acción",
        run: () => uiZoom.smaller(),
      },
      {
        id: "zoom-reset",
        label: t("Interfaz a su tamaño (100 %)"),
        group: "acción",
        run: () => uiZoom.reset(),
      },
      { id: "monitor", label: t("Monitoreo del servidor"), group: "acción", run: onmonitor },
      { id: "config", label: t("Configuración del servidor"), group: "acción", run: onconfig },
      { id: "migrations", label: t("Migraciones del servidor"), group: "acción", run: onmigrations },
      {
        id: "theme",
        label: t("Cambiar el tema"),
        hint: themeLabel(theme.preference),
        group: "acción",
        run: () => theme.cycle(),
      },
    ];

    // Solo tiene sentido con una pestaña de consulta activa: sin editor de CodeMirror a mano, la
    // paleta le pide a la pestaña que formatee y `QueryPanel` es quien la escucha.
    const current = tabs.current;
    if (current instanceof QueryTab) {
      out.push({
        id: "format",
        label: t("Formatear el SQL"),
        hint: shortcutStore.label("editor.format"),
        group: "acción",
        run: () => current.requestFormat(),
      });
      // Lo que solo el editor sabe hacer: la paleta lo pide por nombre y `QueryPanel` se lo pasa.
      for (const item of EDITOR_ACTIONS) {
        out.push({
          id: `editor:${item.action}`,
          // Las etiquetas viven en `editor/actions.ts` (en español): se traducen al armar la paleta.
          label: t(item.label),
          hint: item.shortcut ? shortcutStore.label(item.shortcut) : undefined,
          group: "acción",
          run: () => current.requestEditor(item.action),
        });
      }
      out.push({
        id: "format-options",
        label: t("Opciones del formateador…"),
        hint: t("caja de las palabras clave, sangría, coma"),
        group: "acción",
        run: () => current.requestFormatOptions(),
      });
      out.push({
        id: "versions",
        label: t("Versiones anteriores de este texto"),
        hint: shortcutStore.label("editor.versions"),
        group: "acción",
        run: () => current.requestVersions(),
      });
    }

    for (const item of panes) {
      out.push({
        id: `pane:${item.value}`,
        label: t("Ir a {name}", { name: item.label }),
        group: "vista",
        run: () => view.show(item.value),
      });
    }

    for (const tab of tabs.all) {
      out.push({
        id: `tab:${tab.key}`,
        label: tab.title,
        hint: tab.database,
        group: "pestaña",
        // Ya no hace falta llevar a ninguna vista: las pestañas están siempre a la vista.
        run: () => tabs.activate(tab.key),
      });
    }

    for (const server of explorer.servers) {
      out.push({
        id: `server:${server.profileId}`,
        label: server.label,
        hint: server.connected ? t("conectado") : t("conectar"),
        group: "servidor",
        run: () => {
          if (server.connected) {
            explorer.select(server);
            view.show("explorer");
          } else {
            onconnect(server.profileId);
          }
        },
      });
    }

    for (const row of loadedRows()) {
      out.push({
        id: `row:${row.key}`,
        // La ruta va en la pista y no en el nombre: dos tablas que se llaman igual en esquemas
        // distintos se distinguen sin que el nombre deje de ser lo que se busca.
        label: row.label,
        // La nota va en la pista: es lo que hace que `#facturación` encuentre el objeto.
        hint: [
          [row.node?.database, row.node?.schema].filter(Boolean).join(" / "),
          row.node?.schema && row.node.database
            ? noteHint(notes.get(row.profileId, row.node.database, row.node.schema, row.node.label))
            : "",
        ]
          .filter(Boolean)
          .join(" · "),
        group: "objeto",
        run: () => {
          explorer.select(row);
          view.show("explorer");
        },
      });
    }

    return out;
  });

  const results = $derived(rank(commands, query));

  // Al cambiar lo escrito, la elección vuelve arriba: si no, queda apuntando a una fila que ya no
  // está y Enter corre cualquier cosa.
  $effect(() => {
    void query;
    cursor = 0;
  });

  $effect(() => {
    input?.focus();
  });

  function pick(command: Command | undefined) {
    if (!command) return;
    onclose();
    command.run();
  }

  function onkeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        onclose();
        break;
      case "ArrowDown":
        event.preventDefault();
        cursor = results.length === 0 ? 0 : (cursor + 1) % results.length;
        break;
      case "ArrowUp":
        event.preventDefault();
        cursor = results.length === 0 ? 0 : (cursor - 1 + results.length) % results.length;
        break;
      case "Enter":
        event.preventDefault();
        pick(results[cursor]);
        break;
    }
  }

  function groupLabel(group: CommandGroup): string {
    switch (group) {
      case "acción":
        return t("acción");
      case "vista":
        return t("vista");
      case "servidor":
        return t("servidor");
      case "pestaña":
        return t("pestaña");
      default:
        return t("objeto");
    }
  }

  const GROUP_ICON: Record<CommandGroup, "play" | "compass" | "server" | "sql" | "table"> = {
    acción: "play",
    vista: "compass",
    servidor: "server",
    pestaña: "sql",
    objeto: "table",
  };
</script>

<!-- Cerrar al hacer clic afuera, al revés que `Modal`: acá no hay nada escrito que perder, y una
     paleta que hay que cerrar con un botón deja de ser más rápida que el camino largo. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-40 flex justify-center bg-zinc-950/40 p-4 pt-[12vh] backdrop-blur-[2px]"
  onclick={(event) => event.target === event.currentTarget && onclose()}
  onkeydown={onkeydown}
>
  <div class="card flex max-h-[70vh] w-full max-w-xl flex-col overflow-hidden shadow-2xl">
    <div class="divider-b flex items-center gap-2 px-3 py-2">
      <Icon name="search" size={14} class="shrink-0 text-zinc-400" />
      <input
        bind:this={input}
        bind:value={query}
        class="w-full bg-transparent text-sm outline-none placeholder:text-zinc-400"
        placeholder={t("Escribí una acción, un servidor, una pestaña o una tabla")}
      />
      <kbd class="shrink-0 text-[10px] muted">Esc</kbd>
    </div>

    <div class="min-h-0 flex-1 overflow-auto py-1">
      {#each results as command, index (command.id)}
        <button
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm
                 {index === cursor ? 'bg-blue-500/10' : ''}"
          onclick={() => pick(command)}
          onmouseenter={() => (cursor = index)}
        >
          <Icon name={GROUP_ICON[command.group]} size={12} class="shrink-0 muted" />
          <span class="min-w-0 flex-1 truncate">{command.label}</span>
          {#if command.hint}
            <span class="shrink-0 text-xs muted">{command.hint}</span>
          {/if}
          <span class="w-16 shrink-0 text-right text-[10px] tracking-wide muted uppercase">
            {groupLabel(command.group)}
          </span>
        </button>
      {:else}
        <p class="px-3 py-6 text-center text-xs muted">
          {t("Nada coincide con «{query}». Lo que el árbol todavía no trajo se busca desde el explorador.", { query })}
        </p>
      {/each}
    </div>
  </div>
</div>
