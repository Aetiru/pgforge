/**
 * Cuánto espacio le toca a cada mitad del panel dividido (`Tabs.split`), lado a lado.
 *
 * La pareja de pestañas es efímera —vive en `tabs.svelte.ts` y se pierde al cerrar una de las dos—,
 * pero la proporción sí es una preferencia, igual que el alto del editor (`editor.svelte.ts`): se
 * guarda para no tener que volver a acomodarla la próxima vez que se abre un panel al lado.
 */

const RATIO_KEY = "pgforge.tabs.splitRatio";

const MIN_RATIO = 0.2;
const MAX_RATIO = 0.8;

function clampRatio(value: number): number {
  return Math.min(MAX_RATIO, Math.max(MIN_RATIO, value));
}

function storedRatio(): number {
  const value = Number(localStorage.getItem(RATIO_KEY));
  return Number.isFinite(value) && value > 0 ? clampRatio(value) : 0.5;
}

class SplitView {
  /** Proporción de espacio para la mitad principal; el resto es de la que está al lado. */
  ratio = $state(storedRatio());

  set(ratio: number) {
    this.ratio = clampRatio(ratio);
    localStorage.setItem(RATIO_KEY, String(this.ratio));
  }

  reset() {
    this.set(0.5);
  }
}

export const splitView = new SplitView();
