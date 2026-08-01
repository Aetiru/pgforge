<script lang="ts">
  import { untrack } from "svelte";
  import uPlot from "uplot";
  import "uplot/dist/uPlot.min.css";

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

  // La creación depende solo del tamaño: si también leyera `data`, el gráfico se destruiría y se
  // volvería a crear en cada muestra en lugar de actualizarse.
  $effect(() => {
    const element = container;
    const currentWidth = width;
    if (!element || currentWidth === 0) return;

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
          { stroke: "#9ca3af", grid: { show: false }, size: 22 },
          {
            stroke: "#9ca3af",
            grid: { stroke: "#9ca3af22", width: 1 },
            size: 48,
            values: (_, ticks) => ticks.map((value) => tick(value)),
          },
        ],
        series: [
          {},
          { stroke: color, fill: `${color}22`, width: 1.5, points: { show: false } },
        ],
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
    <span class="text-xs muted">{label}</span>
    <span class="font-mono text-sm tabular-nums">{last}</span>
  </div>
  <div bind:this={container} bind:clientWidth={width} style="height: {height}px"></div>
</div>
