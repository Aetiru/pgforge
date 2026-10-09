<script lang="ts">
  import BlockTree from "./BlockTree.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Sql from "./Sql.svelte";
  import { duration } from "./format";
  import type { Backend, BlockNode, Lock } from "./ipc";
  import {
    KIND_LABEL,
    barShare,
    countSessions,
    filterSessions,
    groupSessions,
    isLong,
    kindOf,
    secondsOf,
    sortSessions,
    type GroupBy,
    type SessionKind,
    type SortKey,
  } from "./sessions-view";

  /**
   * El panel de sesiones del monitoreo.
   *
   * Lo que pasa con cada sesión se lee de tres maneras que comparten datos y selección: una tabla
   * con la duración dibujada dentro de la fila, y una línea de tiempo. A la derecha, el detalle de la
   * elegida con su consulta entera. Si alguna sesión retiene a otras, esa cadena va arriba de todo:
   * es lo único del panel que pide una decisión ahora.
   *
   * Cancelar y terminar **no se ejecutan acá**: se piden y la confirmación es del `Dashboard`, que
   * es quien tiene la conexión. Por eso reciben una lista: marcar varias y terminarlas es el mismo
   * pedido que terminar una.
   */
  let {
    backends,
    blocking,
    selectedPid = $bindable(null),
    locks,
    oncancel,
    onterminate,
    onopen,
  }: {
    backends: Backend[];
    blocking: BlockNode[];
    selectedPid: number | null;
    locks: Lock[];
    oncancel: (pids: number[]) => void;
    onterminate: (pids: number[]) => void;
    onopen: (sql: string, database: string | null) => void;
  } = $props();

  const KEY = "pgforge.sessions";

  interface Saved {
    view: "table" | "timeline";
    group: GroupBy;
    threshold: number;
    freeze: boolean;
  }

  function stored(): Saved {
    const fallback: Saved = { view: "table", group: "none", threshold: 30, freeze: true };
    try {
      const raw = JSON.parse(localStorage.getItem(KEY) ?? "null");
      if (!raw) return fallback;
      return {
        view: raw.view === "timeline" ? "timeline" : "table",
        group: raw.group === "app" || raw.group === "user" ? raw.group : "none",
        threshold: typeof raw.threshold === "number" ? raw.threshold : fallback.threshold,
        freeze: raw.freeze !== false,
      };
    } catch {
      return fallback;
    }
  }

  const saved = stored();
  let view = $state(saved.view);
  let group = $state<GroupBy>(saved.group);
  let threshold = $state(saved.threshold);
  let freeze = $state(saved.freeze);

  // Cada preferencia es de la aplicación y no de una pestaña: se mira igual contra cualquier servidor.
  $effect(() => {
    const value: Saved = { view, group, threshold, freeze };
    try {
      localStorage.setItem(KEY, JSON.stringify(value));
    } catch {
      // Sin `localStorage` la preferencia no se recuerda esta vez.
    }
  });

  let kind = $state<SessionKind | "all">("all");
  let text = $state("");
  let sort = $state<{ key: SortKey; descending: boolean }>({ key: "duration", descending: true });
  let marked = $state<Set<number>>(new Set());
  let copied = $state(false);

  /**
   * La lista que se dibuja. Mientras hay una sesión elegida y «congelar» está prendido, es la foto
   * de cuando se la eligió: con una muestra cada dos segundos, las filas se reordenaban por debajo
   * del cursor justo cuando uno estaba leyendo una. El detalle sí sigue en vivo.
   */
  let frozen = $state.raw<Backend[] | null>(null);
  const source = $derived(frozen ?? backends);

  function pick(pid: number | null) {
    selectedPid = pid;
    frozen = pid !== null && freeze ? backends : null;
  }

  // Elegir desde afuera (la pestaña de bloqueos) también congela.
  $effect(() => {
    if (selectedPid !== null && frozen === null && freeze) frozen = backends;
    if (selectedPid === null) frozen = null;
  });

  const counts = $derived(countSessions(source));
  const shown = $derived(sortSessions(filterSessions(source, { kind, text }), sort.key, sort.descending));
  const groups = $derived(groupSessions(shown, group));
  const longest = $derived(Math.max(1, ...source.map(secondsOf)));

  const live = $derived(selectedPid === null ? null : backends.find((b) => b.pid === selectedPid) ?? null);
  const selected = $derived(live ?? (selectedPid === null ? null : (frozen ?? []).find((b) => b.pid === selectedPid) ?? null));
  const gone = $derived(selectedPid !== null && live === null);

  const chain = $derived(blocking);

  /** Se marcan solo las que se pueden terminar: la sesión del propio monitor no. */
  const markable = $derived(shown.filter((b) => !b.isMonitor));
  const allMarked = $derived(markable.length > 0 && markable.every((b) => marked.has(b.pid)));

  function toggleMark(pid: number) {
    const next = new Set(marked);
    if (next.has(pid)) next.delete(pid);
    else next.add(pid);
    marked = next;
  }

  function toggleAll() {
    marked = allMarked ? new Set() : new Set(markable.map((b) => b.pid));
  }

  function sortBy(key: SortKey) {
    sort = sort.key === key ? { key, descending: !sort.descending } : { key, descending: key === "duration" };
  }

  async function copyQuery() {
    if (!selected?.query) return;
    try {
      await navigator.clipboard.writeText(selected.query);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Sin permiso del portapapeles, el texto sigue seleccionable en el detalle.
    }
  }

  const CHIPS: { value: SessionKind | "all"; label: string }[] = [
    { value: "all", label: "Todas" },
    { value: "blocked", label: "Bloqueadas" },
    { value: "itx", label: "Inactivas en tx" },
    { value: "active", label: "Activas" },
    { value: "idle", label: "Inactivas" },
  ];

  const DOT: Record<SessionKind, string> = {
    blocked: "bg-rose-500",
    itx: "bg-amber-500",
    active: "bg-blue-500",
    idle: "bg-zinc-400 dark:bg-zinc-500",
  };

  const BAR: Record<SessionKind, string> = {
    blocked: "bg-rose-500",
    itx: "bg-amber-500",
    active: "bg-blue-500",
    idle: "bg-zinc-400/70 dark:bg-zinc-500/70",
  };

  const HEADERS: { key: SortKey; label: string; align?: "right" }[] = [
    { key: "pid", label: "PID" },
    { key: "app", label: "Origen" },
    { key: "duration", label: "Duración" },
  ];

  /** Las marcas de la escala, con rótulos cortos: «5 min 0 s» no entra entre dos marcas vecinas. */
  const TICKS: [number, string][] = [
    [1, "1 s"],
    [10, "10 s"],
    [60, "1 min"],
    [300, "5 min"],
  ];
</script>

{#snippet row(backend: Backend)}
  {@const k = kindOf(backend)}
  <tr
    class="cursor-pointer border-b border-zinc-100 text-xs hover:bg-zinc-100 dark:border-zinc-800
           dark:hover:bg-zinc-800/70
           {selectedPid === backend.pid ? 'bg-blue-50 dark:bg-blue-950/40' : ''}"
    aria-selected={selectedPid === backend.pid}
    onclick={() => pick(selectedPid === backend.pid ? null : backend.pid)}
  >
    <td class="w-7 pl-2">
      <input
        type="checkbox"
        class="align-middle"
        aria-label="Marcar la sesión {backend.pid}"
        checked={marked.has(backend.pid)}
        disabled={backend.isMonitor}
        onclick={(event) => event.stopPropagation()}
        onchange={() => toggleMark(backend.pid)}
      />
    </td>
    <td class="px-2 py-1.5 font-mono whitespace-nowrap">
      <span class="mr-1.5 inline-block size-2 rounded-full {DOT[k]}" title={KIND_LABEL[k]}></span>{backend.pid}
    </td>
    <td class="max-w-36 truncate px-2 whitespace-nowrap" title="{backend.user ?? '—'} · {backend.applicationName ?? ''}">
      {backend.applicationName || "—"}
      <span class="muted">· {backend.user ?? "?"}</span>
    </td>
    <td class="px-2 whitespace-nowrap">
      <span class="inline-flex items-center gap-2">
        <span class="inline-block h-1.5 w-16 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-700">
          <span
            class="block h-full rounded-full {BAR[k]}"
            style="width: {Math.max(3, barShare(secondsOf(backend), longest) * 100)}%"
          ></span>
        </span>
        <span
          class="w-16 font-mono {isLong(backend, threshold)
            ? 'font-semibold text-amber-600 dark:text-amber-400'
            : ''}"
          title={isLong(backend, threshold) ? `Lleva más de ${duration(threshold)} corriendo` : ""}
        >
          {duration(secondsOf(backend))}
        </span>
      </span>
    </td>
    <!-- La espera ya no es una columna: casi siempre está vacía y se llevaba el ancho de la consulta.
         Cuando existe, va delante del texto; el detalle la trae completa. -->
    <td class="max-w-0 min-w-56 truncate px-2 font-mono muted" title={backend.query ?? ""}>
      {#if backend.waitEventType}
        <span class="mr-1.5 rounded bg-amber-100 px-1 text-[10px] text-amber-800 dark:bg-amber-950 dark:text-amber-300" title="{backend.waitEventType}: {backend.waitEvent ?? ''}">
          {backend.waitEvent ?? backend.waitEventType}
        </span>
      {/if}
      {backend.query?.replace(/\s+/g, " ") ?? "—"}
    </td>
  </tr>
{/snippet}

<div class="flex min-h-0 flex-1 flex-col gap-2 px-4 pb-3">
  <!-- Los filtros dicen cuántas sesiones dejan pasar antes de apretarlos. -->
  <div class="flex flex-wrap items-center gap-1.5">
    {#each CHIPS as chip (chip.value)}
      <button class="chip-toggle" aria-pressed={kind === chip.value} onclick={() => (kind = chip.value)}>
        {chip.label}
        <span class="font-mono font-semibold">{counts[chip.value]}</span>
      </button>
    {/each}

    <input
      class="field ml-auto w-full max-w-56 py-0.5 text-xs"
      placeholder="Buscar usuario, app o consulta"
      aria-label="Buscar sesiones"
      bind:value={text}
    />
  </div>

  <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
    <div class="seg" role="tablist">
      <button class="seg-item" role="tab" aria-selected={view === "table"} onclick={() => (view = "table")}>
        Tabla
      </button>
      <button class="seg-item" role="tab" aria-selected={view === "timeline"} onclick={() => (view = "timeline")}>
        Línea de tiempo
      </button>
    </div>

    <label class="check text-xs" title="Agrupa las filas bajo el nombre de la aplicación o del usuario">
      Agrupar por
      <select class="field py-0.5 text-xs" bind:value={group}>
        <option value="none">—</option>
        <option value="app">Aplicación</option>
        <option value="user">Usuario</option>
      </select>
    </label>

    <label class="check text-xs" title="Marca en ámbar lo que está corriendo hace más que esto">
      Resaltar desde
      <select class="field py-0.5 text-xs" bind:value={threshold}>
        <option value={0}>no resaltar</option>
        <option value={10}>10 s</option>
        <option value={30}>30 s</option>
        <option value={60}>1 min</option>
        <option value={300}>5 min</option>
      </select>
    </label>

    <label class="check text-xs" title="Con una sesión elegida, la lista deja de reordenarse mientras la leés">
      <input type="checkbox" bind:checked={freeze} />
      Congelar al elegir
    </label>

    {#if frozen}
      <button class="tag tag-warn" onclick={() => pick(selectedPid === null ? null : selectedPid)} title="Vuelve a tomar la lista en vivo">
        lista congelada · actualizar
      </button>
    {/if}
  </div>

  {#if marked.size > 0}
    <div class="flex flex-wrap items-center gap-2 rounded-md border border-blue-300 bg-blue-50 px-3 py-1.5 text-xs dark:border-blue-800 dark:bg-blue-950/40">
      <span class="font-medium">{marked.size} {marked.size === 1 ? "sesión marcada" : "sesiones marcadas"}</span>
      <button class="btn btn-sm" onclick={() => oncancel([...marked])}>Cancelar consultas…</button>
      <button class="btn btn-sm btn-danger-ghost" onclick={() => onterminate([...marked])}>Terminar sesiones…</button>
      <button class="btn btn-sm btn-ghost ml-auto" onclick={() => (marked = new Set())}>Limpiar</button>
    </div>
  {/if}

  <!-- Si alguna sesión retiene a otras, es lo primero que se lee. -->
  {#if chain.length > 0}
    <div class="rounded-md border border-rose-300 bg-rose-50/60 px-2 py-1.5 dark:border-rose-900/60 dark:bg-rose-950/20">
      <p class="px-1 pb-1 text-xs font-medium text-rose-700 dark:text-rose-300">
        Cadena de bloqueo · resolvé la de arriba: las de abajo esperan por ella
      </p>
      {#each chain as node (node.pid)}
        <BlockTree {node} onselect={(pid) => pick(pid)} onterminate={(pid) => onterminate([pid])} />
      {/each}
    </div>
  {/if}

  <div class="flex min-h-0 flex-1 gap-3 @container/sessions">
    <div class="card min-h-0 min-w-0 flex-1 overflow-auto">
      {#if shown.length === 0}
        <Empty icon="search" title="Ninguna sesión coincide" hint="Probá con otro filtro o borrá el texto de búsqueda." />
      {:else if view === "table"}
        <table class="w-full border-collapse">
          <thead class="sticky top-0 z-10 bg-zinc-50 dark:bg-zinc-800">
            <tr class="border-b border-zinc-200 text-left text-[11px] font-medium tracking-wide uppercase muted dark:border-zinc-700">
              <th class="w-7 pl-2">
                <input type="checkbox" aria-label="Marcar todas las visibles" checked={allMarked} onchange={toggleAll} />
              </th>
              {#each HEADERS as header (header.key)}
                <th class="px-2 py-1.5">
                  <button class="inline-flex items-center gap-1 uppercase" onclick={() => sortBy(header.key)}>
                    {header.label}
                    {#if sort.key === header.key}
                      <Icon name="chevron" size={9} class={sort.descending ? "rotate-90" : "-rotate-90"} />
                    {/if}
                  </button>
                </th>
              {/each}
              <th class="px-2 py-1.5">Consulta</th>
            </tr>
          </thead>
          <tbody>
            {#each groups as bucket (bucket.key)}
              {#if group !== "none"}
                <tr class="bg-zinc-100/80 dark:bg-zinc-800/80">
                  <td colspan="5" class="px-3 py-1 text-xs font-semibold">
                    {bucket.label}
                    <span class="font-normal muted">
                      · {bucket.items.length} {bucket.items.length === 1 ? "sesión" : "sesiones"}
                      · la más larga {duration(bucket.longest)}
                    </span>
                  </td>
                </tr>
              {/if}
              {#each bucket.items as backend (backend.pid)}
                {@render row(backend)}
              {/each}
            {/each}
          </tbody>
        </table>
      {:else}
        <!-- La línea de tiempo: la barra es proporcional a la duración en escala logarítmica, así
             20 ms y siete minutos entran en la misma pantalla. Las marcas dicen qué escala es. -->
        <div class="grid grid-cols-[9rem_minmax(0,1fr)_4.5rem] gap-3 border-b border-zinc-200 bg-zinc-50 px-3 py-1 text-[10.5px] font-mono muted dark:border-zinc-700 dark:bg-zinc-800">
          <span>Sesión</span>
          <div class="relative h-3">
            {#each TICKS as [seconds, label] (seconds)}
              <span class="absolute -translate-x-1/2 whitespace-nowrap" style="left: {barShare(seconds, longest) * 100}%">{label}</span>
            {/each}
          </div>
          <span class="text-right">Duración</span>
        </div>
        {#each groups as bucket (bucket.key)}
          {#if group !== "none"}
            <div class="bg-zinc-100/80 px-3 py-1 text-xs font-semibold dark:bg-zinc-800/80">
              {bucket.label} <span class="font-normal muted">· {bucket.items.length}</span>
            </div>
          {/if}
          {#each bucket.items as backend (backend.pid)}
            {@const k = kindOf(backend)}
            <button
              class="grid w-full grid-cols-[9rem_minmax(0,1fr)_4.5rem] items-center gap-3 border-b border-zinc-100 px-3 py-1 text-left text-xs hover:bg-zinc-100 dark:border-zinc-800 dark:hover:bg-zinc-800/70
                     {selectedPid === backend.pid ? 'bg-blue-50 dark:bg-blue-950/40' : ''}"
              onclick={() => pick(selectedPid === backend.pid ? null : backend.pid)}
            >
              <span class="truncate font-mono">
                <span class="mr-1.5 inline-block size-2 rounded-full {DOT[k]}"></span>{backend.pid}
                {backend.user ?? ""}
              </span>
              <span class="relative block h-4 rounded-sm bg-zinc-100 dark:bg-zinc-800">
                {#each TICKS as [seconds] (seconds)}
                  <span class="absolute inset-y-0 w-px bg-zinc-200 dark:bg-zinc-700" style="left: {barShare(seconds, longest) * 100}%"></span>
                {/each}
                <span
                  class="absolute inset-y-0 left-0 rounded-sm {BAR[k]}"
                  style="width: {Math.max(1, barShare(secondsOf(backend), longest) * 100)}%"
                ></span>
              </span>
              <span class="text-right font-mono {isLong(backend, threshold) ? 'font-semibold text-amber-600 dark:text-amber-400' : ''}">
                {duration(secondsOf(backend))}
              </span>
            </button>
          {/each}
        {/each}
      {/if}
    </div>

    <!-- El detalle de la elegida. Sin elegir ninguna no ocupa lugar: la tabla se queda con todo el ancho. -->
    {#if selected}
      <aside class="card flex min-h-0 w-80 shrink-0 flex-col gap-2 overflow-auto p-3 text-sm @max-3xl/sessions:w-64">
        <div class="flex items-start gap-2">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5 font-mono text-sm font-semibold">
              <span class="inline-block size-2 rounded-full {DOT[kindOf(selected)]}"></span>
              PID {selected.pid}
            </div>
            <div class="text-xs muted">{KIND_LABEL[kindOf(selected)]}</div>
          </div>
          <button class="btn btn-ghost btn-icon size-6" aria-label="Cerrar el detalle" title="Cerrar el detalle" onclick={() => pick(null)}>
            <Icon name="close" size={11} />
          </button>
        </div>

        {#if gone}
          <p class="text-xs text-amber-700 dark:text-amber-400">Esta sesión ya terminó: lo que se ve es su última muestra.</p>
        {/if}

        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs">
          <dt class="muted">Usuario</dt><dd class="font-mono">{selected.user ?? "—"}@{selected.database ?? "—"}</dd>
          <dt class="muted">Aplicación</dt><dd class="font-mono break-all">{selected.applicationName || "—"}</dd>
          <dt class="muted">Cliente</dt><dd class="font-mono">{selected.clientAddr ?? "local"}</dd>
          <dt class="muted">Consulta</dt><dd class="font-mono">{duration(selected.querySeconds)}</dd>
          <dt class="muted">Transacción</dt><dd class="font-mono">{duration(selected.transactionSeconds)}</dd>
          {#if selected.waitEventType}
            <dt class="muted">Espera</dt><dd class="font-mono">{selected.waitEventType}: {selected.waitEvent ?? ""}</dd>
          {/if}
          {#if selected.blockedBy.length > 0}
            <dt class="muted">Bloqueada por</dt>
            <dd class="font-mono">
              {#each selected.blockedBy as pid, index (pid)}
                {index > 0 ? ", " : ""}<button class="text-rose-600 underline dark:text-rose-400" onclick={() => pick(pid)}>{pid}</button>
              {/each}
            </dd>
          {/if}
        </dl>

        {#if locks.length > 0}
          <p class="text-xs muted" title="Candados que tiene o espera esta sesión">
            Candados: {locks.map((lock) => `${lock.mode}${lock.granted ? "" : " (esperando)"}`).join(", ")}
          </p>
        {/if}

        {#if selected.query}
          <div class="max-h-60 min-h-0 overflow-auto rounded-md bg-zinc-50 px-2 py-1 select-text dark:bg-zinc-900">
            <Sql code={selected.query} />
          </div>
        {/if}

        {#if selected.isMonitor}
          <span class="tag tag-neutral">es la sesión del propio monitor</span>
        {:else}
          <div class="flex flex-wrap gap-1.5">
            {#if selected.query}
              <button class="btn btn-sm" onclick={copyQuery}>
                <Icon name={copied ? "check" : "copy"} size={11} />
                {copied ? "Copiada" : "Copiar"}
              </button>
              <button class="btn btn-sm" onclick={() => onopen(selected.query ?? "", selected.database)}>
                Abrir en una consulta
              </button>
            {/if}
            <button class="btn btn-sm" onclick={() => oncancel([selected.pid])}>Cancelar consulta…</button>
            <button class="btn btn-sm btn-danger-ghost" onclick={() => onterminate([selected.pid])}>Terminar sesión…</button>
          </div>
          <p class="text-[11px] muted">Cancelar y terminar piden confirmación. Terminar revierte la transacción.</p>
        {/if}
      </aside>
    {/if}
  </div>
</div>
