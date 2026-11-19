import { getCurrentWebview } from "@tauri-apps/api/webview";
import { wGet, wSet } from "./wstorage";

/**
 * La escala de toda la interfaz, no solo del SQL: el tamaño de letra del editor y de la grilla
 * (`editor.svelte.ts`) no agranda la barra lateral, los menús ni las pestañas, que en una pantalla
 * de alta resolución quedan chicos. Es el zoom del propio *webview*, que escala todo junto y no
 * rompe la medición de alturas que sí rompería un `zoom` de CSS sobre el documento.
 */
const KEY = "pgforge.ui.zoom";
const MIN = 0.8;
const MAX = 1.5;
const STEP = 0.1;

export function clampZoom(value: number): number {
  if (!Number.isFinite(value)) return 1;
  // Se redondea a décimas: sumar 0,1 varias veces acumula error de coma flotante (1,2000000000000002).
  return Math.round(Math.min(MAX, Math.max(MIN, value)) * 10) / 10;
}

export function storedZoom(raw: string | null): number {
  return clampZoom(raw === null ? 1 : Number(raw));
}

class UiZoom {
  level = $state(storedZoom(wGet(KEY)));

  constructor() {
    if (this.level !== 1) void this.apply();
  }

  private async apply() {
    try {
      await getCurrentWebview().setZoom(this.level);
    } catch {
      // Fuera de una ventana de Tauri (pruebas, vista previa en el navegador) no hay a quién escalar.
    }
  }

  set(level: number) {
    this.level = clampZoom(level);
    try {
      wSet(KEY, String(this.level));
    } catch {
      // Sin almacenamiento, vale hasta cerrar la ventana.
    }
    void this.apply();
  }

  bigger() {
    this.set(this.level + STEP);
  }

  smaller() {
    this.set(this.level - STEP);
  }

  reset() {
    this.set(1);
  }

  get percent(): string {
    return `${Math.round(this.level * 100)} %`;
  }
}

export const uiZoom = new UiZoom();
