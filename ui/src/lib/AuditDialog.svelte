<script lang="ts">
  import Alert from "./Alert.svelte";
  import Empty from "./Empty.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import Sql from "./Sql.svelte";
  import { activeFilters, EMPTY_FORM, exportName, toFilter, type AuditForm } from "./audit-filter";
  import { envLook } from "./badges";
  import { decimal } from "./format";
  import {
    auditExport,
    auditFacets,
    auditQuery,
    describeError,
    pickSave,
    type AuditFacets,
    type AuditFormat,
    type Environment,
    type HistoryEntry,
  } from "./ipc";

  /**
   * El registro de auditoría: lo que la aplicación ejecutó, con quién, contra qué servidor y cuándo.
   *
   * Es el historial visto con filtros, y exportable. Dos límites que el pie lo dice sin vueltas: es
   * el registro **de pgforge** —no ve lo que otra herramienta corrió contra la misma base— y el
   * «usuario» es el rol con el que se conectó el perfil, no una persona.
   */
  let { onclose }: { onclose: () => void } = $props();

  const PAGE = 200;

  let form = $state<AuditForm>({ ...EMPTY_FORM });
  let facets = $state<AuditFacets>({ servers: [], databases: [] });
  let entries = $state<HistoryEntry[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let selectedId = $state<number | null>(null);
  const selected = $derived(entries.find((entry) => entry.id === selectedId) ?? null);
  let timer: ReturnType<typeof setTimeout> | null = null;
  /** Descarta la respuesta de una lectura vieja si ya se pidió otra: llegan fuera de orden. */
  let sequence = 0;

  function serverLabel(entry: HistoryEntry): string {
    if (entry.serverName) return entry.serverName;
    const known = facets.servers.find((server) => server.profileId === entry.profileId);
    return known?.serverName ?? entry.profileId.slice(0, 8);
  }

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleString("es", {
      day: "2-digit",
      month: "2-digit",
      year: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
  }

  async function load(append = false) {
    const mine = ++sequence;
    loading = true;
    error = null;
    try {
      const page = await auditQuery(toFilter(form), PAGE, append ? entries.length : 0);
      if (mine !== sequence) return;
      entries = append ? [...entries, ...page.entries] : page.entries;
      total = page.total;
    } catch (failure) {
      if (mine === sequence) error = describeError(failure);
    } finally {
      if (mine === sequence) loading = false;
    }
  }

  /** El texto se busca tras una pausa; el resto de los filtros, al cambiar. */
  function textSoon() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void load(), 300);
  }

  function clear() {
    form = { ...EMPTY_FORM };
    void load();
  }

  async function exportTo(format: AuditFormat) {
    notice = null;
    error = null;
    try {
      const path = await pickSave({
        title: "Exportar el registro de auditoría",
        defaultPath: exportName(format),
        filters: [
          format === "csv"
            ? { name: "CSV", extensions: ["csv"] }
            : { name: "JSON", extensions: ["json"] },
        ],
      });
      if (!path) return;
      const rows = await auditExport(toFilter(form), format, path);
      notice = `${rows} fila${rows === 1 ? "" : "s"} exportada${rows === 1 ? "" : "s"} a ${path}`;
    } catch (failure) {
      error = describeError(failure);
    }
  }

  $effect(() => {
    auditFacets()
      .then((value) => (facets = value))
      .catch((failure) => (error = describeError(failure)));
    void load();
  });
</script>

<Modal title="Registro de auditoría" subtitle="{total} ejecuciones" size="2xl" onclose={onclose}>
  <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
    <label class="flex flex-col gap-1">
      <span class="label">Servidor</span>
      <select class="field" bind:value={form.profileId} onchange={() => load()}>
        <option value="">Todos</option>
        {#each facets.servers as server (server.profileId)}
          <option value={server.profileId}>{server.serverName ?? server.profileId.slice(0, 8)}</option>
        {/each}
      </select>
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Base</span>
      <select class="field" bind:value={form.database} onchange={() => load()}>
        <option value="">Todas</option>
        {#each facets.databases as database (database)}
          <option value={database}>{database}</option>
        {/each}
      </select>
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Desde</span>
      <input class="field" type="date" bind:value={form.from} onchange={() => load()} />
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Hasta</span>
      <input class="field" type="date" bind:value={form.to} onchange={() => load()} />
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Origen</span>
      <select class="field" bind:value={form.source} onchange={() => load()}>
        <option value="">Todos</option>
        <option value="editor">Editor de consultas</option>
        <option value="dialog">Diálogos de la aplicación</option>
      </select>
    </label>
    <label class="flex flex-col gap-1">
      <span class="label">Entorno</span>
      <select class="field" bind:value={form.environment} onchange={() => load()}>
        <option value="">Todos</option>
        <option value="prod">prod</option>
        <option value="test">test</option>
        <option value="dev">dev</option>
      </select>
    </label>
    <label class="col-span-2 flex flex-col gap-1">
      <span class="label">Texto en el SQL o el error</span>
      <input
        class="field"
        placeholder="ALTER TABLE, DROP, permission denied…"
        bind:value={form.text}
        oninput={textSoon}
      />
    </label>
  </div>

  <div class="mt-2 flex flex-wrap items-center gap-2">
    <label class="check">
      <input type="checkbox" bind:checked={form.onlyFailed} onchange={() => load()} />
      Solo las que fallaron
    </label>
    <button class="btn btn-sm btn-ghost" disabled={activeFilters(form) === 0} onclick={clear}>
      Limpiar filtros{activeFilters(form) > 0 ? ` (${activeFilters(form)})` : ""}
    </button>
    <span class="ml-auto"></span>
    <button
      class="btn btn-sm"
      disabled={total === 0}
      title="Exporta todo lo que cumple el filtro, no solo lo que se ve en la lista"
      onclick={() => exportTo("csv")}
    >
      <Icon name="download" size={11} /> CSV
    </button>
    <button class="btn btn-sm" disabled={total === 0} onclick={() => exportTo("json")}>
      <Icon name="download" size={11} /> JSON
    </button>
  </div>

  {#if error}
    <Alert tone="bad" box class="mt-2" onclose={() => (error = null)}>{error}</Alert>
  {/if}
  {#if notice}
    <Alert tone="ok" box class="mt-2" onclose={() => (notice = null)}>
      <span class="select-text">{notice}</span>
    </Alert>
  {/if}

  <div class="mt-3 max-h-[45vh] overflow-auto rounded-md border border-zinc-200 dark:border-zinc-700">
    {#if entries.length === 0 && !loading}
      <Empty
        icon="clock"
        title="Sin ejecuciones"
        hint="Ninguna ejecución registrada cumple estos filtros."
      />
    {:else}
      <table class="list-table">
        <thead>
          <tr>
            <th>Cuándo</th>
            <th>Servidor</th>
            <th>Base</th>
            <th>Usuario</th>
            <th>Origen</th>
            <th>Resultado</th>
            <th>SQL</th>
          </tr>
        </thead>
        <tbody>
          {#each entries as entry (entry.id)}
            <tr
              class="cursor-pointer {entry.id === selectedId ? 'bg-blue-50 dark:bg-blue-950/50' : ''}"
              onclick={() => (selectedId = entry.id)}
            >
              <td class="text-xs whitespace-nowrap tabular-nums">{when(entry.startedAt)}</td>
              <td class="text-xs whitespace-nowrap">
                {serverLabel(entry)}
                {#if entry.environment === "dev" || entry.environment === "test" || entry.environment === "prod"}
                  {@const look = envLook(entry.environment as Environment)}
                  <span class="tag {look.tone} ml-1">{look.label}</span>
                {/if}
              </td>
              <td class="text-xs whitespace-nowrap">{entry.database}</td>
              <td class="font-mono text-xs whitespace-nowrap">{entry.dbUser ?? "—"}</td>
              <td class="text-xs whitespace-nowrap">{entry.source === "dialog" ? "diálogo" : "editor"}</td>
              <td class="text-xs whitespace-nowrap">
                {#if entry.succeeded}
                  <span class="tag tag-ok">ok</span>
                  <span class="muted tabular-nums">{decimal(entry.seconds * 1000, 0)} ms</span>
                {:else}
                  <span class="tag tag-bad" title={entry.error ?? ""}>falló</span>
                {/if}
              </td>
              <td class="max-w-[22rem] truncate font-mono text-xs">{entry.sql.replace(/\s+/g, " ")}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  {#if entries.length < total}
    <button class="btn btn-sm mt-2" disabled={loading} onclick={() => load(true)}>
      {#if loading}<span class="spinner"></span>{/if}
      Cargar {Math.min(PAGE, total - entries.length)} más ({entries.length} de {total})
    </button>
  {/if}

  {#if selected}
    <div class="mt-3 max-h-48 overflow-auto rounded-md border border-zinc-200 p-2 select-text dark:border-zinc-700">
      <Sql code={selected.sql} />
      {#if selected.error}
        <p class="mt-2 text-xs text-rose-600 dark:text-rose-400">{selected.error}</p>
      {/if}
    </div>
  {/if}

  <p class="mt-3 text-xs muted">
    Es el registro de pgforge en esta máquina: no incluye lo que otra herramienta haya ejecutado
    contra el mismo servidor. El usuario es el rol con el que se conectó el servidor, no una persona.
    Las contraseñas de <span class="font-mono">CREATE/ALTER ROLE</span> se guardan ocultas.
  </p>

  {#snippet footer()}
    <button class="btn ml-auto" onclick={onclose}>Cerrar</button>
  {/snippet}
</Modal>
