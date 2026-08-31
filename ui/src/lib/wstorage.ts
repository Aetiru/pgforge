/**
 * `localStorage` acotado a la ventana actual.
 *
 * Cada ventana de Tauri —la principal (`main`) y la de cada workspace (`ws-<id>`, ver
 * `workspace_open` del núcleo)— es un *webview* aparte, pero las dos comparten el mismo origen y por
 * lo tanto el mismo `localStorage`: sin esto, dos ventanas abiertas a la vez se pisan qué carpeta
 * quedó cerrada o cuánto mide el panel dividido, cada una escribiendo encima de lo que dejó la otra.
 *
 * `main` es la única con historia: escribía sin prefijo desde antes de que existieran los
 * workspaces, así que a falta de la clave nueva cae a la vieja, para no perder la preferencia de
 * quien ya tenía la aplicación instalada; al escribir usa siempre el prefijo nuevo, la clave vieja
 * se deja como estaba. Una ventana de workspace no tiene ese pasado —es una ventana recién abierta—
 * y arranca sin nada guardado, que es lo esperable.
 */

import { getCurrentWindow } from "@tauri-apps/api/window";

/** El label de la ventana actual, o `null` fuera de una ventana de Tauri (los tests corren en Node). */
function windowLabel(): string | null {
  try {
    return getCurrentWindow().label;
  } catch {
    return null;
  }
}

export function wGet(key: string): string | null {
  try {
    const label = windowLabel();
    if (label === null) return localStorage.getItem(key);

    const value = localStorage.getItem(`${label}:${key}`);
    if (value !== null) return value;
    // Solo `main` tiene una clave vieja sin prefijo a la que caer; una ventana de workspace no
    // escribió nada antes de existir.
    return label === "main" ? localStorage.getItem(key) : null;
  } catch {
    // Ni un valor corrupto ni la falta de `localStorage` —los tests corren en Node— pueden impedir
    // que esto se cargue.
    return null;
  }
}

export function wSet(key: string, value: string): void {
  try {
    const label = windowLabel();
    localStorage.setItem(label === null ? key : `${label}:${key}`, value);
  } catch {
    // Nada que hacer sin `localStorage`: la preferencia no se recuerda esta vez.
  }
}
