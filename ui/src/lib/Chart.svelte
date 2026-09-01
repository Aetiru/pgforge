<script lang="ts">
  import { untrack } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  import Icon from "./Icon.svelte";
  import { theme } from "./theme.svelte";

  let {
    label,
    data,
    color = "#3b82f6",
    height = 84,
    formatValue = (value: number) => value.toFixed(0),
    formatTick,
  }: {
    label: string;
    /** `[tiempos, valores]`, en el formato alineado que espera uPlot. */
    data: uPlot.AlignedData;
    color?: string;
    height?: number;
    /** Formato del número grande del encabezado. */
    formatValue?: (value: number) => string;
    /**
     * Formato de las marcas del eje. Por omisión se redondea: uPlot escribe todos los decimales
     * que necesita para distinguir dos marcas, y un `74.5923` no entra en el ancho del eje.
     */
    formatTick?: (value: number) => string;
  } = $props();

  const tick = (value: number) =>
    formatTick ? formatTick(value) : String(Math.round(value * 10) / 10);

  let container = $state<HTMLDivElement | null>(null);
  let width = $state(0);
  let plot: uPlot | null = null;

  const rawLast = $derived.by(() => {
    const values = data[1] as (number | null | undefined)[] | undefined;
    const value = values?.at(-1);
    return value === null || value === undefined ? null : value;
  });

  const last = $derived(rawLast === null ? "—" : formatValue(rawLast));

  /**
   * Sube, baja o se mantiene contra la muestra anterior. Sin color de bueno/malo: subir es lo
   * esperable en «Transacciones/s» y lo contrario en «Esperando», y el gráfico no sabe cuál es.
   */
  const trend = $derived.by(() => {
    const values = data[1] as (number | null | undefined)[] | undefined;
    const previous = values?.at(-2);
    if (rawLast === null || previous === null || previous === undefined) return null;
    if (rawLast === previous) return "flat" as const;
    return rawLast > previous ? ("up" as const) : ("down" as const);
  });

  /** uPlot recibe colores, no clases: los toma del mismo lugar que el resto de la interfaz. */
  function cssColor(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  // La creación depende del tamaño y del tema: si también leyera `data`, el gráfico se destruiría
  // y se volvería a crear en cada muestra en lugar de actualizarse. Los colores de los ejes se
  // fijan al construirlo, así que cambiar de claro a oscuro sí obliga a rehacerlo.
  $effect(() => {
    const element = container;
    const currentWidth = width;
    void theme.resolved;
    if (!element || currentWidth === 0) return;

    const axis = cssColor("--plot-axis");
    const grid = cssColor("--plot-grid");

    plot = new uPlot(
      {
        width: currentWidth,
        height,
        padding: [8, 8, 0, 0],
        // El dashboard se refresca solo; un cursor que sigue al mouse agrega trabajo por cuadro
        // sin aportar nada a un gráfico de esta altura.
        cursor: { show: false },
        legend: { show: false },
        scales: { x: { time: true } },
        axes: [
          { stroke: axis, grid: { show: false }, size: 22 },
          {
            stroke: axis,
            grid: { stroke: grid, width: 1 },
            size: 48,
            values: (_, ticks) => ticks.map((value) => tick(value)),
          },
        ],
        series: [{}, { stroke: color, fill: `${color}22`, width: 1.5, points: { show: false } }],
      },
      // Se arranca con lo que haya, pero sin registrar `data` como dependencia de este efecto.
      untrack(() => data),
      element,
    );

    return () => {
      plot?.destroy();
      plot = null;
    };
  });

  $effect(() => {
    const current = data;
    plot?.setData(current);
  });
</script>

<div class="group flex flex-col gap-2 px-3.5 py-3 transition-colors hover:bg-zinc-50 dark:hover:bg-white/[0.03]">
  <div class="flex items-center justify-between gap-2">
    <span class="flex items-center gap-1.5 text-[11px] font-medium tracking-wide muted">
      <span
        class="size-1.5 rounded-full"
        style="background: {color}; box-shadow: 0 0 0 3px {color}22"
      ></span>
      {label}
    </span>
    {#if trend && trend !== "flat"}
      <Icon
        name="chevron"
        size={10}
        class="shrink-0 text-zinc-400 dark:text-zinc-500 {trend === 'up' ? '-rotate-90' : 'rotate-90'}"
      />
    {/if}
  </div>

  <div class="font-mono text-2xl leading-snug font-semibold tabular-nums" style="color: {color}">
    {last}
  </div>

  <div
    bind:this={container}
    bind:clientWidth={width}
    class="-mx-1.5 opacity-90 transition-opacity group-hover:opacity-100"
    style="height: {height}px"
  ></div>
</div>
