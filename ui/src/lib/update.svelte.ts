/**
 * El cartel de versión nueva: cuándo se pregunta y cuándo se muestra.
 *
 * Dos reglas, las dos de esta máquina y por eso en `localStorage` como el tema o el tamaño de tanda:
 *
 * - **Cada cuánto se pregunta.** No en cada arranque: quien abre la aplicación diez veces al día
 *   haría diez pedidos a una API que sin autenticar permite 60 por hora y por dirección IP, para
 *   enterarse de algo que cambia cada varias semanas.
 * - **Qué versión ya se descartó.** «Ahora no» silencia esa versión y no el aviso entero: la
 *   siguiente vuelve a avisar, que es justo lo que el usuario aceptó al descartar una sola.
 *
 * Que falle la comprobación no se le muestra a nadie. Es lo más accesorio de la aplicación, y un
 * cartel rojo porque no había internet al abrir es peor que no enterarse de la versión nueva.
 */

import { updateCheck, updateOpen, type Release } from "./ipc";

const LAST_KEY = "pgforge.update.lastCheck";
const DISMISSED_KEY = "pgforge.update.dismissed";

/** Un día. Una release no sale más seguido que eso. */
export const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

/** Si toca volver a preguntar. Pura para poder probarla sin reloj ni red. */
export function shouldCheck(lastCheck: number | null, now: number): boolean {
  if (lastCheck === null) return true;
  // Un reloj movido hacia atrás dejaría la última comprobación en el futuro y el aviso mudo para
  // siempre: cualquier cosa que no sea un intervalo razonable hacia atrás vuelve a preguntar.
  if (lastCheck > now) return true;
  return now - lastCheck >= CHECK_INTERVAL_MS;
}

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    // Sin `localStorage` —los tests corren en Node— se pregunta siempre y no se recuerda nada
    // descartado, que es el comportamiento aceptable si falta el almacenamiento.
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Nada que hacer: el aviso vuelve a aparecer la próxima vez.
  }
}

class Updates {
  /** La versión más nueva encontrada, o `null` si no hay ninguna o ya se descartó. */
  release = $state<Release | null>(null);

  /** Abierto mientras se miran las notas de la versión. */
  showing = $state(false);

  /**
   * Pregunta a GitHub si corresponde. Se llama al arrancar la aplicación; con `force` —el botón de
   * «buscar actualizaciones»— se saltea el intervalo y también lo que se haya descartado.
   */
  async check(force = false) {
    const now = Date.now();
    const last = Number(read(LAST_KEY));

    if (!force && !shouldCheck(Number.isFinite(last) && last > 0 ? last : null, now)) return;

    try {
      const result = await updateCheck();
      write(LAST_KEY, String(now));

      const newer = result.newer ?? null;
      this.release = newer && (force || read(DISMISSED_KEY) !== newer.version) ? newer : null;
      if (force) this.showing = this.release !== null;
    } catch {
      // A propósito en silencio: ver el comentario de arriba.
    }
  }

  /** Silencia esta versión. La próxima vuelve a avisar. */
  dismiss() {
    if (this.release) write(DISMISSED_KEY, this.release.version);
    this.release = null;
    this.showing = false;
  }

  /** Abre la página de la release en el navegador del sistema. */
  async open() {
    const url = this.release?.url;
    if (url) await updateOpen(url);
  }
}

export const updates = new Updates();
