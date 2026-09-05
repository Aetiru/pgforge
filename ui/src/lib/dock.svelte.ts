/**
 * La geometría de la carcasa: qué paneles laterales están abiertos y cuánto miden.
 *
 * Vive fuera de `App.svelte` porque ya no es cosa suya: el riel de la izquierda abre el panel
 * lateral, el inspector de la derecha se pliega desde la barra de pestañas y los dos se recuerdan
 * entre sesiones. Es la misma decisión que el alto del panel de resultados (`editor.svelte.ts`) o
 * la proporción del panel dividido (`split-view.svelte.ts`): acomodar la ventana una vez por
 * sesión no es una preferencia que valga la pena repetir.
 */

import { wGet, wSet } from "./wstorage";

const SIDEBAR_OPEN = "pgforge.dock.sidebarOpen";
const SIDEBAR_WIDTH = "pgforge.dock.sidebarWidth";
const INSPECTOR_OPEN = "pgforge.dock.inspectorOpen";
const INSPECTOR_WIDTH = "pgforge.dock.inspectorWidth";
const LIBRARY_OPEN = "pgforge.dock.libraryOpen";
const LIBRARY_HEIGHT = "pgforge.dock.libraryHeight";

export const SIDEBAR_DEFAULT = 300;
export const INSPECTOR_DEFAULT = 400;
export const LIBRARY_DEFAULT = 220;

const SIDEBAR_MIN = 220;
const SIDEBAR_MAX = 560;
/**
 * El inspector llega más lejos que el árbol: adentro se lee el DDL de una tabla, que es texto y no
 * una lista de nombres. Su mínimo también es más alto por lo mismo — la tabla de propiedades queda
 * ilegible antes de eso.
 */
const INSPECTOR_MIN = 300;
const INSPECTOR_MAX = 900;
/**
 * La biblioteca fija es una sección chica al pie del panel lateral, no otro panel entero: el mínimo
 * deja ver un puñado de filas, y el máximo no le puede ganar el lugar al árbol que tiene arriba.
 */
const LIBRARY_MIN = 120;
const LIBRARY_MAX = 500;

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function storedWidth(key: string, fallback: number, min: number, max: number): number {
  const value = Number(wGet(key));
  return Number.isFinite(value) && value > 0 ? clamp(value, min, max) : fallback;
}

/** A falta de valor guardado los dos paneles arrancan abiertos: es la distribución que se ve. */
function storedOpen(key: string): boolean {
  return wGet(key) !== "0";
}

class Dock {
  sidebarOpen = $state(storedOpen(SIDEBAR_OPEN));
  sidebarWidth = $state(
    storedWidth(SIDEBAR_WIDTH, SIDEBAR_DEFAULT, SIDEBAR_MIN, SIDEBAR_MAX),
  );
  inspectorOpen = $state(storedOpen(INSPECTOR_OPEN));
  inspectorWidth = $state(
    storedWidth(
      INSPECTOR_WIDTH,
      INSPECTOR_DEFAULT,
      INSPECTOR_MIN,
      INSPECTOR_MAX,
    ),
  );
  /** La sección fija de marcadores y scripts arranca abierta: es la que la hace útil todos los días. */
  libraryOpen = $state(storedOpen(LIBRARY_OPEN));
  libraryHeight = $state(storedWidth(LIBRARY_HEIGHT, LIBRARY_DEFAULT, LIBRARY_MIN, LIBRARY_MAX));

  setSidebar(open: boolean) {
    this.sidebarOpen = open;
    wSet(SIDEBAR_OPEN, open ? "1" : "0");
  }

  toggleSidebar() {
    this.setSidebar(!this.sidebarOpen);
  }

  setSidebarWidth(width: number) {
    this.sidebarWidth = clamp(width, SIDEBAR_MIN, SIDEBAR_MAX);
    wSet(SIDEBAR_WIDTH, String(this.sidebarWidth));
  }

  setInspector(open: boolean) {
    this.inspectorOpen = open;
    wSet(INSPECTOR_OPEN, open ? "1" : "0");
  }

  toggleInspector() {
    this.setInspector(!this.inspectorOpen);
  }

  setInspectorWidth(width: number) {
    this.inspectorWidth = clamp(width, INSPECTOR_MIN, INSPECTOR_MAX);
    wSet(INSPECTOR_WIDTH, String(this.inspectorWidth));
  }

  setLibrary(open: boolean) {
    this.libraryOpen = open;
    wSet(LIBRARY_OPEN, open ? "1" : "0");
  }

  toggleLibrary() {
    this.setLibrary(!this.libraryOpen);
  }

  setLibraryHeight(height: number) {
    this.libraryHeight = clamp(height, LIBRARY_MIN, LIBRARY_MAX);
    wSet(LIBRARY_HEIGHT, String(this.libraryHeight));
  }
}

export const dock = new Dock();
