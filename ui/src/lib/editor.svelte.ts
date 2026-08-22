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
 * Si el editor y los resultados se reparten el alto, o uno de los dos se queda con todo.
 *
 * Escribir una consulta larga contra una pantalla de portátil dejaba el editor en un tercio del
 * alto, con el resto ocupado por una grilla vacía o por el resultado de la corrida anterior — de ahí
 * `"sql"`. Mirar un resultado ancho de muchas columnas pedía lo mismo al revés — de ahí `"rows"`. Son
 * tres estados y no dos booleanos porque son excluyentes: el editor y la grilla no pueden estar los
 * dos plegados a la vez, y dos interruptores independientes permitirían justamente ese estado que no
 * significa nada.
 *
 * Va con el alto y no por pestaña: es la misma decisión —cuánto espacio quiero para escribir o para
 * mirar— y tenerla que repetir en cada consulta nueva es justo lo que la haría inservible.
 */
export type SplitMode = "split" | "sql" | "rows";

const SPLIT_MODE_KEY = "pgforge.sql.split";
/** Clave vieja, de cuando plegar los resultados era el único modo que existía. */
const LEGACY_HIDDEN_KEY = "pgforge.sql.resultsHidden";

function storedMode(): SplitMode {
  const value = localStorage.getItem(SPLIT_MODE_KEY);
  if (value === "split" || value === "sql" || value === "rows") return value;
  // Sin la clave nueva, se respeta la preferencia vieja en vez de perderla al actualizar.
  return localStorage.getItem(LEGACY_HIDDEN_KEY) === "on" ? "sql" : "split";
}

class EditorSplit {
  height = $state(storedHeight());
  mode = $state(storedMode());

  set(height: number) {
    this.height = clampHeight(height);
    localStorage.setItem(SPLIT_KEY, String(this.height));
  }

  reset() {
    this.set(DEFAULT_EDITOR_HEIGHT);
  }

  private setMode(mode: SplitMode) {
    this.mode = mode;
    localStorage.setItem(SPLIT_MODE_KEY, mode);
  }

  get resultsHidden() {
    return this.mode === "sql";
  }

  get editorHidden() {
    return this.mode === "rows";
  }

  /** Pliega los resultados y le deja todo el alto al editor; repetirlo vuelve al reparto normal. */
  toggleResults() {
    this.setMode(this.mode === "sql" ? "split" : "sql");
  }

  /** Pliega el editor y le deja todo el alto a la grilla; repetirlo vuelve al reparto normal. */
  toggleEditor() {
    this.setMode(this.mode === "rows" ? "split" : "rows");
  }
}

export const editorSplit = new EditorSplit();

/**
 * Formatear el SQL solo, sin que lo pidan con `Ctrl+Mayús+F` o el botón: antes de guardar y antes
 * de ejecutar el script entero (`saveQueryTab`, «ejecutar todo»).
 *
 * Apagado por omisión: formatear sin que lo pidan es la clase de cosa que enoja, y que se pueda
 * apagar es la mitad de la función. No toca la ejecución de una sola sentencia (`Ctrl+Enter`) — ahí
 * se está iterando sobre una consulta, y reescribir el documento entero en cada corrida es
 * exactamente lo que hace que la gente lo apague.
 */
const AUTO_FORMAT_KEY = "pgforge.sql.autoFormat";

function storedAutoFormat(): boolean {
  return localStorage.getItem(AUTO_FORMAT_KEY) === "on";
}

class AutoFormat {
  enabled = $state(storedAutoFormat());

  set(enabled: boolean) {
    this.enabled = enabled;
    localStorage.setItem(AUTO_FORMAT_KEY, enabled ? "on" : "off");
  }
}

export const autoFormat = new AutoFormat();
