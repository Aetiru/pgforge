import { invoke } from "./core";

/**
 * Diálogos nativos de archivos, abiertos **desde Rust**.
 *
 * Reemplazan al complemento de JavaScript (`@tauri-apps/plugin-dialog`) a propósito: lo que el
 * usuario elige acá queda autorizado del lado de Rust, y es lo único que después aceptan los
 * comandos que reciben una ruta (`sqlReadFile`, `dataExportRun`, …). Una ruta escrita a mano, o
 * armada por código que no sea un diálogo, se rechaza. No hay —ni debe haber— una función que
 * «autorice» una ruta que ya está en el webview.
 *
 * La firma imita a la del complemento (`open`/`save`) para que quien migra solo cambie el nombre.
 */

export interface DialogFilter {
  name: string;
  extensions: string[];
}

export interface DialogOptions {
  title?: string;
  /** Orienta dónde abre y qué nombre propone; no autoriza nada por sí mismo. */
  defaultPath?: string;
  filters?: DialogFilter[];
  directory?: boolean;
  multiple?: boolean;
}

/**
 * Elegir un archivo o una carpeta. `null` si se cancela; con `multiple` devuelve la lista, y sin
 * él, la única ruta.
 */
export async function pickOpen(
  options: DialogOptions & { multiple: true },
): Promise<string[] | null>;
export async function pickOpen(options?: DialogOptions): Promise<string | null>;
export async function pickOpen(options: DialogOptions = {}): Promise<string | string[] | null> {
  const chosen = await invoke<string[]>("dialog_open", { options });
  if (chosen.length === 0) return null;
  return options.multiple ? chosen : chosen[0];
}

/** Elegir dónde guardar. `null` si se cancela. */
export const pickSave = (options: DialogOptions = {}) =>
  invoke<string | null>("dialog_save", { options });
