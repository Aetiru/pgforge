/**
 * Tamaño de la grilla de resultados.
 *
 * Vale para las tres grillas del proyecto —el resultado de una consulta, la pestaña de datos y el
 * dashboard—: es la misma grilla leída con los mismos ojos, y tener un tamaño por lugar obligaría a
 * ajustarlos de a uno. Se guarda, como el tamaño de letra del SQL, porque quien la agranda no lo hace
 * por un rato.
 *
 * El alto de fila sale de la letra y no es un mando aparte: son dos caras de lo mismo —qué tan densa
 * se lee la grilla—, y dos controles independientes obligarían a ajustar de a uno lo que en realidad
 * es una sola decisión. `+10` deja la fila en 24 px con la letra por omisión, que es el alto con el
 * que la grilla venía antes de este cambio.
 */

const KEY = "pgforge.grid.font";

/** El tamaño con el que nació la grilla (el `text-sm` de siempre), y al que vuelve el botón central. */
export const DEFAULT_GRID_FONT = 14;
const MIN = 10;
const MAX = 22;

function clamp(size: number): number {
  return Math.min(MAX, Math.max(MIN, Math.round(size)));
}

function stored(): number {
  const value = Number(localStorage.getItem(KEY));
  return Number.isFinite(value) && value > 0 ? clamp(value) : DEFAULT_GRID_FONT;
}

class GridZoom {
  size = $state(stored());

  constructor() {
    this.apply();
  }

  /** Alto de fila en píxeles, para la ventana deslizante de `DataGrid`. */
  get rowHeight() {
    return this.size + 10;
  }

  private apply() {
    document.documentElement.style.setProperty("--grid-font-size", `${this.size}px`);
    // El encabezado siempre un punto más chico que el cuerpo, como el `text-xs` de hoy contra el
    // `text-sm` de las filas: la relación entre los dos no es una preferencia, es parte del diseño.
    document.documentElement.style.setProperty("--grid-header-font-size", `${this.size - 1}px`);
  }

  set(size: number) {
    this.size = clamp(size);
    localStorage.setItem(KEY, String(this.size));
    this.apply();
  }

  /** De a un píxel: el salto de dos ya se siente como otra grilla. */
  bigger() {
    this.set(this.size + 1);
  }

  smaller() {
    this.set(this.size - 1);
  }

  reset() {
    this.set(DEFAULT_GRID_FONT);
  }

  get canGrow() {
    return this.size < MAX;
  }

  get canShrink() {
    return this.size > MIN;
  }
}

export const gridZoom = new GridZoom();
