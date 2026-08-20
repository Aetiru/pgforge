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

/**
 * Alto del editor dentro de la pestaña de consulta.
 *
 * Se guarda por la misma razón que el tamaño de letra, y **es una sola preferencia para todas las
 * pestañas**: era un `$state` local de cada `QueryPanel`, así que agrandar el editor duraba hasta
 * abrir la consulta siguiente, que volvía a nacer chica con el resultado ocupando casi todo.
 */
const SPLIT_KEY = "pgforge.sql.editorHeight";

export const DEFAULT_EDITOR_HEIGHT = 320;
const MIN_HEIGHT = 96;
const MAX_HEIGHT = 720;

function clampHeight(height: number): number {
  return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.round(height)));
}

function storedHeight(): number {
  const value = Number(localStorage.getItem(SPLIT_KEY));
  return Number.isFinite(value) && value > 0 ? clampHeight(value) : DEFAULT_EDITOR_HEIGHT;
}

/**
 * Si el panel de resultados está plegado.
 *
 * Escribir una consulta larga contra una pantalla de portátil dejaba el editor en un tercio del
 * alto, con el resto ocupado por una grilla vacía o por el resultado de la corrida anterior. Se
 * pliega y el editor se queda con todo; la barra de resultados sigue a la vista, porque un panel que
 * se esconde sin dejar de dónde agarrarlo no se vuelve a abrir.
 *
 * Va con el alto y no por pestaña: es la misma decisión —cuánto espacio quiero para escribir— y
 * tenerla que repetir en cada consulta nueva es justo lo que la haría inservible.
 */
const HIDDEN_KEY = "pgforge.sql.resultsHidden";

class EditorSplit {
  height = $state(storedHeight());
  hidden = $state(localStorage.getItem(HIDDEN_KEY) === "on");

  set(height: number) {
    this.height = clampHeight(height);
    localStorage.setItem(SPLIT_KEY, String(this.height));
  }

  reset() {
    this.set(DEFAULT_EDITOR_HEIGHT);
  }

  toggle() {
    this.hidden = !this.hidden;
    localStorage.setItem(HIDDEN_KEY, this.hidden ? "on" : "off");
  }
}

export const editorSplit = new EditorSplit();
