<script lang="ts">
  import { kindLabel } from "./badges";
  import { explorer } from "./explorer.svelte";
  import { describeError, folderOf, objectDdl, type Ddl } from "./ipc";

  let ddl = $state<Ddl | null>(null);
  let ddlError = $state<string | null>(null);
  let loading = $state(false);
  let copied = $state(false);

  const selected = $derived(explorer.selected);
  const node = $derived(selected?.node ?? null);
  /** Ni las carpetas ni la fila del servidor tienen un DDL propio que mostrar. */
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
</script>

<div class="flex h-full flex-col">
  {#if !selected}
    <div class="flex h-full items-center justify-center p-6 text-sm text-neutral-500">
      Elegí un objeto del árbol.
    </div>
  {:else}
    <header class="border-b border-neutral-200 px-4 py-3 dark:border-neutral-800">
      <div class="flex items-baseline gap-2">
        <h2 class="truncate text-base font-medium">{selected.label}</h2>
        <span class="text-xs text-neutral-500">{kindLabel(node?.kind ?? null)}</span>
      </div>
      {#if node}
        <p class="mt-0.5 text-xs text-neutral-500">
          {node.database}{node.schema ? ` · ${node.schema}` : ""}{node.oid
            ? ` · oid ${node.oid}`
            : ""}
        </p>
      {/if}
      {#if selected.comment}
        <p class="mt-2 text-sm text-neutral-600 dark:text-neutral-300">{selected.comment}</p>
      {/if}
    </header>

    <div class="min-h-0 flex-1 overflow-auto">
      {#if !hasDdl}
        <p class="p-4 text-sm text-neutral-500">Este nodo no tiene un DDL propio.</p>
      {:else if loading}
        <p class="p-4 text-sm text-neutral-500">Generando DDL…</p>
      {:else if ddlError}
        <p class="p-4 text-sm text-red-600 dark:text-red-400">{ddlError}</p>
      {:else if ddl}
        <div class="flex items-center justify-between px-4 py-2 text-xs text-neutral-500">
          <span>
            {ddl.source === "pgDump"
              ? "Reconstruido con pg_dump"
              : "Generado por PostgreSQL"}
          </span>
          <button
            class="rounded border border-neutral-300 px-2 py-0.5 hover:bg-neutral-100
                   dark:border-neutral-700 dark:hover:bg-neutral-800"
            onclick={copy}
          >
            {copied ? "Copiado" : "Copiar"}
          </button>
        </div>
        <pre
          class="select-text overflow-x-auto px-4 pb-4 font-mono text-xs leading-relaxed">{ddl.sql}</pre>
      {/if}
    </div>
  {/if}
</div>
