<script lang="ts">
  import { untrack } from "svelte";
  import { bytes, count } from "./format";
  import { dataOpen, type TableShape, type TreeNode } from "./ipc";
  import { lookOf } from "./badges";
  import Icon from "./Icon.svelte";
  import { notes } from "./notes.svelte";

  /**
   * La tarjeta que aparece al dejar el cursor sobre una relación del árbol: lo que se quiere saber
   * antes de decidir si vale abrirla —cuánto pesa, qué clave tiene, qué columnas— sin abrirla.
   *
   * No recibe clics (`pointer-events-none`): es para mirar, y si estorbara al mouse al pasar a la
   * fila de al lado sería peor que no tenerla. La forma de la tabla se pide una vez y se recuerda un
   * rato; sin ella la tarjeta igual muestra lo que el árbol ya trajo.
   */
  let {
    profileId,
    node,
    top = 0,
    left = 0,
    inline = false,
  }: {
    profileId: string;
    node: TreeNode;
    top?: number;
    left?: number;
    /** Dentro de otro globo (el del editor): sin posición propia, que ya la pone quien lo contiene. */
    inline?: boolean;
  } = $props();

  const MAX_COLUMNS = 8;
  const CACHE_MS = 60_000;
  const cache: Map<string, { at: number; shape: Promise<TableShape> }> =
    (globalThis as { __pgforgePeek?: Map<string, { at: number; shape: Promise<TableShape> }> }).__pgforgePeek ??
    ((globalThis as { __pgforgePeek?: unknown }).__pgforgePeek = new Map());

  let shape = $state<TableShape | null>(null);

  $effect(() => {
    const { oid, database } = untrack(() => ({ oid: node.oid, database: node.database }));
    if (oid === undefined) return;

    const key = `${profileId}:${database}:${oid}`;
    let entry = cache.get(key);
    if (!entry || Date.now() - entry.at > CACHE_MS) {
      entry = { at: Date.now(), shape: dataOpen(profileId, oid, database) };
      cache.set(key, entry);
    }
    let cancelled = false;
    entry.shape
      .then((loaded) => {
        if (!cancelled) shape = loaded;
      })
      .catch(() => cache.delete(key));
    return () => {
      cancelled = true;
    };
  });

  const look = $derived(lookOf(node.kind));
  const note = $derived(
    node.schema ? notes.get(profileId, node.database, node.schema, node.label) : null,
  );
  const keyColumns = $derived(new Set(shape?.key?.columns ?? []));
</script>

<div
  class="card pointer-events-none w-72 p-3 text-xs {inline ? '' : 'fixed z-50 shadow-xl'}"
  style={inline ? undefined : `top: ${top}px; left: ${left}px`}
  role="tooltip"
>
  <div class="flex items-center gap-1.5">
    <Icon name={look.icon} class={look.tone} size={14} />
    <span class="min-w-0 truncate font-mono text-[13px] font-semibold">{node.label}</span>
    <span class="ml-auto shrink-0 muted">{node.schema}</span>
  </div>

  {#if node.comment}
    <p class="mt-1.5 text-zinc-600 dark:text-zinc-300">{node.comment}</p>
  {/if}

  <div class="mt-2 flex flex-wrap gap-x-3 gap-y-0.5 tabular-nums muted">
    {#if node.rows !== undefined}<span>~{count(node.rows)} filas</span>{/if}
    {#if node.sizeBytes !== undefined}<span>{bytes(node.sizeBytes)}</span>{/if}
    {#if shape?.key}<span>clave: {shape.key.columns.join(", ")}</span>{/if}
    {#if shape && !shape.key && !shape.readOnly}<span>sin clave</span>{/if}
    {#if shape && shape.referencedBy.length > 0}<span>{shape.referencedBy.length} la referencian</span>{/if}
  </div>

  {#if note}
    <div class="mt-2 border-t border-zinc-200 pt-1.5 dark:border-zinc-700">
      {#if note.tags.length > 0}
        <div class="flex flex-wrap gap-1">
          {#each note.tags as tag (tag)}<span class="tag tag-info">#{tag}</span>{/each}
        </div>
      {/if}
      {#if note.note}<p class="mt-1 line-clamp-3">{note.note}</p>{/if}
    </div>
  {/if}

  <div class="mt-2 border-t border-zinc-200 pt-1.5 dark:border-zinc-700">
    {#if shape}
      {#each shape.columns.slice(0, MAX_COLUMNS) as column (column.name)}
        <div class="flex items-baseline gap-2 leading-5">
          <span class="min-w-0 truncate font-mono {keyColumns.has(column.name) ? 'font-semibold' : ''}">
            {column.name}
          </span>
          <span class="ml-auto shrink-0 truncate font-mono text-[10px] muted">{column.typeName}</span>
        </div>
      {/each}
      {#if shape.columns.length > MAX_COLUMNS}
        <div class="muted">+{shape.columns.length - MAX_COLUMNS} columnas más</div>
      {/if}
    {:else}
      <div class="skeleton h-3 w-40"></div>
    {/if}
  </div>
</div>
