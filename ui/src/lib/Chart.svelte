<script lang="ts">
  import { untrack } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";
  import { theme } from "./theme.svelte";

  let {
    label,
    data,
    color = "#3b82f6",
    height = 92,
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

  const last = $derived.by(() => {
    const values = data[1] as (number | null | undefined)[] | undefined;
    const value = values?.at(-1);
    return value === null || value === undefined ? "—" : formatValue(value);
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

<div class="card p-2">
  <div class="flex items-baseline justify-between px-1">
    <span class="flex items-center gap-1.5 text-xs muted">
      <span class="size-1.5 rounded-full" style="background: {color}"></span>
      {label}
    </span>
    <span class="font-mono text-sm tabular-nums">{last}</span>
  </div>
  <div bind:this={container} bind:clientWidth={width} style="height: {height}px"></div>
</div>
