<script lang="ts">
  import Icon from "./Icon.svelte";
  import { kindLabel, lookOf } from "./badges";
  import { explorer } from "./explorer.svelte";
  import { describeError, folderOf, formatVersion, objectDdl, type Ddl } from "./ipc";

  let {
    onedit,
    ondelete,
    onconnect,
    onquery,
  }: {
    onedit: (profileId: string) => void;
    ondelete: (profileId: string) => void;
    onconnect: (profileId: string) => void;
    onquery: (profileId: string, database: string, title: string) => void;
  } = $props();

  let ddl = $state<Ddl | null>(null);
  let ddlError = $state<string | null>(null);
  let loading = $state(false);
  let copied = $state(false);

  const selected = $derived(explorer.selected);
  const node = $derived(selected?.node ?? null);
  const isServer = $derived(selected !== null && selected.node === null);
  const profile = $derived(
    selected ? (explorer.profiles.find((item) => item.id === selected.profileId) ?? null) : null,
  );
  const caps = $derived(selected ? (explorer.caps[selected.profileId] ?? null) : null);
  const look = $derived(lookOf(node?.kind ?? null));

  /** Ni las carpetas, ni las bases, ni la fila del servidor tienen un DDL propio. */
  const hasDdl = $derived(node !== null && folderOf(node.kind) === null && node.kind !== "database");

  $effect(() => {
    const current = node;
    ddl = null;
    ddlError = null;
    copied = false;

    if (!current || !hasDdl || !selected) return;

    const profileId = selected.profileId;
    let cancelled = false;
    loading = true;

    objectDdl(profileId, current)
      .then((result) => {
        if (!cancelled) ddl = result;
      })
      .catch((error) => {
        if (!cancelled) ddlError = describeError(error);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });

    // Cambiar de nodo rápido no debe dejar que una respuesta vieja pise a la nueva.
    return () => {
      cancelled = true;
    };
  });

  async function copy() {
    if (!ddl) return;
    await navigator.clipboard.writeText(ddl.sql);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  /**
   * Contra qué base abriría una consulta lo que está seleccionado. Los objetos la llevan encima;
   * la fila del servidor recién conectado usa la del perfil.
   */
  const queryTarget = $derived.by<{ database: string; title: string } | null>(() => {
    if (!selected) return null;
    if (node) return { database: node.database, title: node.label };
    if (selected.connected && profile) {
      return { database: profile.database, title: profile.name };
    }
    return null;
  });

  const properties = $derived.by<[string, string][]>(() => {
    if (isServer && profile) {
      const rows: [string, string][] = [
        ["Servidor", `${profile.host}:${profile.port}`],
        ["Base inicial", profile.database],
        ["Usuario", profile.user],
        ["Cifrado", profile.sslMode],
      ];
      if (caps) {
        rows.push(
          ["Versión", `PostgreSQL ${formatVersion(caps.version)}`],
          ["Superusuario", caps.isSuperuser ? "sí" : "no"],
          [
            "Puede cancelar sesiones",
            caps.canSignalBackends ? "sí" : "no (falta pg_signal_backend)",
          ],
          [
            "Ve todas las estadísticas",
            caps.canReadAllStats ? "sí" : "no (falta pg_read_all_stats)",
          ],
        );
      }
      return rows;
    }

    if (!node) return [];
    const rows: [string, string][] = [["Base de datos", node.database]];
    if (node.schema) rows.push(["Esquema", node.schema]);
    if (node.oid) rows.push(["OID", String(node.oid)]);
    return rows;
  });
</script>

<div class="flex h-full flex-col">
  {#if !selected}
    <div class="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      <Icon name="schema" size={28} class="text-zinc-300 dark:text-zinc-700" />
      <p class="text-sm muted">Elegí un objeto del árbol para ver su detalle.</p>
    </div>
  {:else}
    <header class="divider-b px-5 py-4">
      <div class="flex items-center gap-2">
        <Icon name={look.icon} size={18} class={look.tone} />
        <h2 class="truncate text-base font-medium">{selected.label}</h2>
        <span class="tag tag-neutral">{kindLabel(node?.kind ?? null)}</span>

        {#if queryTarget}
          <button
            class="btn ml-auto shrink-0"
            title={`Abre una consulta contra ${queryTarget.database}`}
            onclick={() =>
              onquery(selected.profileId, queryTarget.database, queryTarget.title)}
          >
            <Icon name="sql" size={12} />
            Consulta
          </button>
        {/if}

        {#if isServer}
          <span class="flex shrink-0 gap-1.5 {queryTarget ? '' : 'ml-auto'}">
            {#if selected.connected}
              <button class="btn" onclick={() => explorer.disconnect(selected.profileId)}>
                Desconectar
              </button>
            {:else}
              <button class="btn btn-primary" onclick={() => onconnect(selected.profileId)}>
                Conectar
              </button>
            {/if}
            <button class="btn" onclick={() => onedit(selected.profileId)}>Editar</button>
            <button class="btn" onclick={() => ondelete(selected.profileId)}>Eliminar</button>
          </span>
        {/if}
      </div>

      {#if selected.comment}
        <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">{selected.comment}</p>
      {/if}
    </header>

    <div class="min-h-0 flex-1 overflow-auto p-5">
      {#if properties.length > 0}
        <dl class="mb-5 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
          {#each properties as [label, value] (label)}
            <dt class="muted">{label}</dt>
            <dd class="truncate">{value}</dd>
          {/each}
        </dl>
      {/if}

      {#if isServer && !selected.connected}
        <p class="text-sm muted">Conectá el servidor para explorar sus objetos.</p>
      {:else if !hasDdl && !isServer}
        <p class="text-sm muted">Este nodo agrupa otros objetos; no tiene un DDL propio.</p>
      {:else if hasDdl}
        <div class="card overflow-hidden">
          <div class="divider-b flex items-center gap-2 px-3 py-1.5">
            <span class="text-xs font-medium">DDL</span>
            {#if ddl}
              <span class="text-xs muted">
                {ddl.source === "pgDump" ? "reconstruido con pg_dump" : "generado por PostgreSQL"}
              </span>
              <button class="btn btn-ghost ml-auto px-2 py-0.5 text-xs" onclick={copy}>
                <Icon name="copy" size={12} />
                {copied ? "Copiado" : "Copiar"}
              </button>
            {/if}
          </div>

          {#if loading}
            <p class="px-3 py-4 text-sm muted">Generando DDL…</p>
          {:else if ddlError}
            <p class="px-3 py-4 text-sm text-rose-600 dark:text-rose-400">{ddlError}</p>
          {:else if ddl}
            <pre
              class="max-h-[60vh] overflow-auto px-3 py-3 font-mono text-xs leading-relaxed
                     select-text">{ddl.sql}</pre>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
