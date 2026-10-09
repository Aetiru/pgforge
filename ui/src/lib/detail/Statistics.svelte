<script lang="ts">
  /** Cómo está la tabla: tamaño, filas vivas y muertas, último vacuum y análisis, y cómo se la lee. */
  import Icon from "../Icon.svelte";
  import { ago, bytes, count } from "../format";
  import type { TableStat } from "../ipc";
  import { lastVacuumSeconds, tableHints } from "../table-health";
  import Card from "./Card.svelte";

  let {
    stat,
    loading = false,
    error = null,
    partitioned = false,
    blocked = {},
    onmaintenance,
  }: {
    stat: TableStat | null;
    loading?: boolean;
    error?: string | null;
    partitioned?: boolean;
    blocked?: { disabled?: boolean; title?: string };
    onmaintenance: () => void;
  } = $props();

  const hints = $derived(stat ? tableHints(stat) : []);
  const deadPercent = $derived(stat?.deadRatio == null ? 0 : stat.deadRatio * 100);
  const indexScans = $derived(stat?.indexScans ?? null);
</script>

{#snippet figure(label: string, value: string, hint?: string)}
  <div class="flex flex-col gap-0.5 px-3 py-2" title={hint}>
    <span class="text-[11px] muted">{label}</span>
    <span class="font-mono text-base font-semibold tabular-nums">{value}</span>
  </div>
{/snippet}

<Card
  title="Estadísticas"
  loading={loading ? "Leyendo estadísticas…" : null}
  {error}
  empty={!loading && !error && !stat
    ? partitioned
      ? "Una tabla particionada no guarda filas propias: mirá las estadísticas de cada partición."
      : "El servidor no lleva estadísticas de esta tabla."
    : null}
>
  {#snippet actions()}
    {#if stat}
      <button class="btn btn-sm ml-auto" {...blocked} onclick={onmaintenance} title="VACUUM, ANALYZE o REINDEX sobre esta tabla">
        <Icon name="gauge" size={11} />
        Mantenimiento…
      </button>
    {/if}
  {/snippet}

  {#if stat}
    {#if hints.length > 0}
      <ul class="flex flex-col gap-1 px-3 pt-2">
        {#each hints as hint, index (index)}
          <li
            class="flex items-start gap-2 rounded-md px-2 py-1 text-xs
              {hint.level === 'warn'
              ? 'bg-amber-50 text-amber-800 dark:bg-amber-950/40 dark:text-amber-300'
              : 'bg-zinc-100 text-zinc-700 dark:bg-zinc-800 dark:text-zinc-300'}"
          >
            <Icon name={hint.level === "warn" ? "warn" : "info"} size={12} class="mt-0.5 shrink-0" />
            {hint.text}
          </li>
        {/each}
      </ul>
    {/if}

    <div class="grid grid-cols-2 divide-x divide-y divide-zinc-200 border-b border-zinc-200 dark:divide-zinc-700 dark:border-zinc-700 @md/detail:grid-cols-4">
      {@render figure("Tamaño total", bytes(stat.totalBytes), "Tabla, índices y TOAST")}
      {@render figure("Solo la tabla", bytes(stat.tableBytes), "Incluye el TOAST")}
      {@render figure("Índices", bytes(stat.indexBytes))}
      {@render figure("Filas vivas", count(stat.liveTuples), "Estimación de las estadísticas, no un recuento")}
    </div>

    <div class="flex flex-col gap-1 px-3 py-2">
      <div class="flex items-baseline justify-between text-xs">
        <span class="muted">Filas muertas</span>
        <span class="font-mono tabular-nums">
          {count(stat.deadTuples)}
          <span class="muted">({deadPercent < 10 ? deadPercent.toFixed(1) : Math.round(deadPercent)} %)</span>
        </span>
      </div>
      <div class="h-1.5 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-700">
        <div
          class="h-full rounded-full {deadPercent >= 20 ? 'bg-amber-500' : 'bg-blue-500'}"
          style="width: {Math.min(100, Math.max(deadPercent, stat.deadTuples > 0 ? 1 : 0))}%"
        ></div>
      </div>
      <p class="text-[11px] muted">
        Una estimación sobre los contadores de estadísticas, no una medición del espacio desperdiciado.
      </p>
    </div>

    <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 border-t border-zinc-200 px-3 py-2 text-xs dark:border-zinc-700">
      <dt class="muted">Último VACUUM</dt>
      <dd class="font-mono">{ago(lastVacuumSeconds(stat))}</dd>
      <dt class="muted">Último ANALYZE</dt>
      <dd class="font-mono">{ago(stat.lastAnalyzeSeconds)}</dd>
      <dt class="muted">Lecturas secuenciales</dt>
      <dd class="font-mono">{count(stat.sequentialScans)}</dd>
      <dt class="muted">Lecturas por índice</dt>
      <dd class="font-mono">{indexScans === null ? "—" : count(indexScans)}</dd>
    </dl>
  {/if}
</Card>
