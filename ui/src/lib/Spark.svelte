<script lang="ts">
  /**
   * Una línea de tendencia sin ejes ni etiquetas, para acompañar un número.
   *
   * Es un SVG a mano y no un `uPlot`: una ficha del dashboard tiene lugar para una forma, no para
   * un gráfico, y montar un uPlot por ficha costaría más de lo que muestra. Los huecos (`null`, un
   * contador que todavía no tiene dos muestras) cortan la línea en vez de bajarla a cero.
   */
  let { values, width = 64, height = 22 }: { values: (number | null)[]; width?: number; height?: number } =
    $props();

  const points = $derived.by(() => {
    const numeric = values.filter((value): value is number => value !== null);
    if (numeric.length < 2) return null;
    const min = Math.min(...numeric);
    const max = Math.max(...numeric);
    // Sin variación la línea va al medio: pegada al fondo parecería un cero.
    const span = max - min || 1;
    const pad = 2;
    const step = (width - pad * 2) / (values.length - 1);
    const y = (value: number) =>
      max === min ? height / 2 : height - pad - ((value - min) / span) * (height - pad * 2);

    const segments: string[] = [];
    let current = "";
    values.forEach((value, index) => {
      if (value === null) {
        if (current) segments.push(current);
        current = "";
        return;
      }
      current += `${current ? "L" : "M"}${(pad + index * step).toFixed(1)} ${y(value).toFixed(1)}`;
    });
    if (current) segments.push(current);

    const last = values.length - 1;
    const lastValue = values[last];
    return {
      path: segments.join(" "),
      end: lastValue === null ? null : { x: pad + last * step, y: y(lastValue) },
    };
  });
</script>

{#if points}
  <svg {width} {height} viewBox="0 0 {width} {height}" class="shrink-0 text-blue-500 dark:text-blue-400" aria-hidden="true">
    <path d={points.path} fill="none" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round" />
    {#if points.end}
      <circle cx={points.end.x} cy={points.end.y} r="2" fill="currentColor" />
    {/if}
  </svg>
{/if}
