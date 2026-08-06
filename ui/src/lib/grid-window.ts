/**
 * Qué columnas de la grilla hay que dibujar.
 *
 * Vive fuera del componente porque es donde el desplazamiento se rompe de formas que no se ven
 * mirando la pantalla quieta: una columna de más a la izquierda deja un hueco al llegar al borde, y
 * una de menos hace parpadear la que está entrando. Acá se prueba con números en vez de arrastrando
 * la barra a mano.
 */

export interface ColumnRange {
  /** Primera y última columna a dibujar, ambas incluidas. `last < first` significa ninguna. */
  first: number;
  last: number;
}

/**
 * Las columnas que asoman en `[scrollLeft, scrollLeft + viewportWidth)`, más `overscan` a cada lado.
 *
 * `offsets[i]` es dónde empieza la columna `i`, y `widths[i]` cuánto mide. Se recorren de a una en
 * vez de buscarlas por bisección porque son decenas, no miles: el costo del desplazamiento está en
 * las celdas que se dibujan, no en encontrarlas.
 */
export function columnRange(
  offsets: number[],
  widths: number[],
  scrollLeft: number,
  viewportWidth: number,
  overscan: number,
): ColumnRange {
  const count = offsets.length;
  if (count === 0) return { first: 0, last: -1 };

  // La primera que todavía no terminó antes del borde izquierdo.
  let first = 0;
  while (first < count - 1 && offsets[first] + widths[first] <= scrollLeft) first += 1;

  // La última que empieza antes del borde derecho.
  let last = first;
  while (last < count - 1 && offsets[last + 1] < scrollLeft + viewportWidth) last += 1;

  return {
    first: Math.max(0, first - overscan),
    last: Math.min(count - 1, last + overscan),
  };
}
