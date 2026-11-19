<script lang="ts">
  /**
   * Una fila vista de arriba abajo: una línea por columna, con el valor entero.
   *
   * La grilla recorta cada celda a una línea y, con veinte columnas, la mitad queda fuera de la
   * pantalla. Esto es lo mismo que hace el panel de detalle con las columnas de la tabla, pero con
   * los valores de la fila elegida. Es de solo lectura: editar sigue pasando por la grilla, que es
   * la que lleva el control de cambios pendientes.
   */
  import { boolText, isBoolType } from "./format";
  import { geometryText, isGeometryType } from "./geometry";
  import Icon from "./Icon.svelte";
  import type { DataTab, Row } from "./data.svelte";

  let { tab, row, onclose }: { tab: DataTab; row: Row | null; onclose: () => void } = $props();

  let needle = $state("");
  let copied = $state<string | null>(null);

  const fields = $derived.by(() => {
    const shape = tab.shape;
    if (!shape || !row) return [];
    const wanted = needle.trim().toLowerCase();
    return shape.columns
      .map((column, index) => {
        const raw = tab.value(row, index);
        let text: string | null = raw;
        if (raw !== null && isBoolType(column.typeName)) text = boolText(raw);
        else if (raw !== null && isGeometryType(column.typeName)) text = geometryText(raw);
        return { name: column.name, type: column.typeName, raw, text, edited: row.edited.has(index) };
      })
      .filter((field) => wanted === "" || field.name.toLowerCase().includes(wanted));
  });

  async function copy(name: string, value: string | null) {
    if (value === null) return;
    await navigator.clipboard.writeText(value).catch(() => {});
    copied = name;
    setTimeout(() => (copied = copied === name ? null : copied), 1200);
  }
</script>

<aside class="flex h-full w-80 shrink-0 flex-col border-l border-zinc-200 dark:border-zinc-700">
  <div class="divider-b flex items-center gap-1.5 px-3 py-1.5">
    <span class="text-xs font-medium">
      {row ? `Fila ${row.index + 1}` : "Fila"}
    </span>
    <span class="text-[11px] muted">{tab.shape?.columns.length ?? 0} columnas</span>
    <button class="btn btn-ghost btn-icon ml-auto" aria-label="Cerrar" onclick={onclose}>
      <Icon name="close" size={11} />
    </button>
  </div>

  {#if !row}
    <p class="p-3 text-xs muted">Elegí una fila de la grilla para ver todos sus valores.</p>
  {:else}
    <div class="divider-b px-2 py-1">
      <input
        class="field w-full py-0.5 text-xs"
        placeholder="Filtrar columnas…"
        bind:value={needle}
      />
    </div>
    <dl class="min-h-0 flex-1 overflow-auto select-text">
      {#each fields as field (field.name)}
        <div class="group/field border-b border-zinc-100 px-3 py-1.5 dark:border-zinc-800">
          <dt class="flex items-baseline gap-1.5 text-[11px]">
            <span class="font-medium {field.edited ? 'text-amber-700 dark:text-amber-400' : ''}">
              {field.name}
            </span>
            <span class="muted">{field.type}</span>
            <button
              class="ml-auto opacity-0 group-hover/field:opacity-100 focus-visible:opacity-100"
              disabled={field.raw === null}
              title="Copiar el valor"
              aria-label="Copiar {field.name}"
              onclick={() => copy(field.name, field.raw)}
            >
              <Icon name={copied === field.name ? "check" : "copy"} size={11} />
            </button>
          </dt>
          <!-- Valor entero, con saltos de línea y sin recorte; un JSON o un texto largo se desplaza
               adentro de su caja en vez de empujar al resto. -->
          <dd class="mt-0.5 max-h-40 overflow-auto font-mono text-xs break-words whitespace-pre-wrap">
            {#if field.text === null}
              <span class="cell-null">NULL</span>
            {:else}
              {field.text}
            {/if}
          </dd>
        </div>
      {:else}
        <p class="p-3 text-xs muted">Ninguna columna coincide.</p>
      {/each}
    </dl>
  {/if}
</aside>
