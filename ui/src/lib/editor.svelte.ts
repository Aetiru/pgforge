/**
 * Tamaño de letra del SQL.
 *
 * Vale para todo lo que muestra SQL —el editor de consultas, el DDL de un objeto, la vista previa de
 * una mutación—: es la misma letra leída con los mismos ojos, y tener un tamaño por lugar obligaría a
 * ajustarlos de a uno. Se guarda, como el tema, porque quien lo agranda no lo hace por un rato.
 *
 * Lo que se aplica es una variable CSS en la raíz, no una clase por componente: CodeMirror se
 * configura con temas propios y así el tamaño lo decide un solo lugar, igual que los colores.
 */

const KEY = "pgforge.sql.font";

/** El tamaño con el que nació el editor, y al que vuelve `Ctrl 0`. */
export const DEFAULT_SQL_FONT = 13;
const MIN = 10;
const MAX = 24;

function clamp(size: number): number {
  return Math.min(MAX, Math.max(MIN, Math.round(size)));
}

function stored(): number {
  const value = Number(localStorage.getItem(KEY));
  return Number.isFinite(value) && value > 0 ? clamp(value) : DEFAULT_SQL_FONT;
}

class SqlFont {
  size = $state(stored());

  constructor() {
    this.apply();
  }

  private apply() {
    document.documentElement.style.setProperty("--sql-font-size", `${this.size}px`);
  }

  set(size: number) {
    this.size = clamp(size);
    localStorage.setItem(KEY, String(this.size));
    this.apply();
  }

  /** De a un píxel: el salto de dos ya se siente como otro editor. */
  bigger() {
    this.set(this.size + 1);
  }

  smaller() {
    this.set(this.size - 1);
  }

  reset() {
    this.set(DEFAULT_SQL_FONT);
  }

  get canGrow() {
    return this.size < MAX;
  }

  get canShrink() {
    return this.size > MIN;
  }
}

export const sqlFont = new SqlFont();
