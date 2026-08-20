<script lang="ts">
  import Icon from "./Icon.svelte";
  import { explorer, type Row } from "./explorer.svelte";
  import { rank, type Command, type CommandGroup } from "./palette";
  import { tabs } from "./tabs.svelte";
  import { theme } from "./theme.svelte";
  import { view, type MainView } from "./view.svelte";

  /**
   * La paleta de comandos.
   *
   * En una aplicación con árbol, pestañas, cuatro vistas y treinta diálogos, lo que más tiempo come
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
    onconnect,
    onclose,
  }: {
    onnewquery: () => void;
    onopensql: () => void;
    onnewserver: () => void;
    onconnect: (profileId: string) => void;
    onclose: () => void;
  } = $props();

  const THEME_LABEL = { system: "el del sistema", light: "claro", dark: "oscuro" } as const;

  let query = $state("");
  let cursor = $state(0);
  let input = $state<HTMLInputElement | null>(null);

  const VIEWS: { value: MainView; label: string }[] = [
    { value: "explorer", label: "Explorador" },
    { value: "monitor", label: "Monitoreo" },
    { value: "config", label: "Configuración del servidor" },
    { value: "processes", label: "Procesos" },
  ];

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
        label: "Nueva consulta",
        hint: "Ctrl+Q",
        group: "acción",
        run: onnewquery,
      },
      { id: "sql", label: "Abrir un archivo SQL", hint: "Ctrl+O", group: "acción", run: onopensql },
      { id: "server", label: "Nuevo servidor", group: "acción", run: onnewserver },
      {
        id: "theme",
        label: "Cambiar el tema",
        hint: THEME_LABEL[theme.preference],
        group: "acción",
        run: () => theme.cycle(),
      },
    ];

    for (const item of VIEWS) {
      out.push({
        id: `view:${item.value}`,
        label: `Ir a ${item.label}`,
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
        run: () => {
          tabs.active = tab.key;
          view.show("explorer");
        },
      });
    }

    for (const server of explorer.servers) {
      out.push({
        id: `server:${server.profileId}`,
        label: server.label,
        hint: server.connected ? "conectado" : "conectar",
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
        hint: [row.node?.database, row.node?.schema].filter(Boolean).join(" / "),
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
        placeholder="Escribí una acción, un servidor, una pestaña o una tabla"
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
            {command.group}
          </span>
        </button>
      {:else}
        <p class="px-3 py-6 text-center text-xs muted">
          Nada coincide con «{query}». Lo que el árbol todavía no trajo se busca desde el explorador.
        </p>
      {/each}
    </div>
  </div>
</div>
