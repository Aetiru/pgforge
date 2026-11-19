/**
 * Atrás y adelante entre las pestañas que se fueron mirando, como en un navegador.
 *
 * Es distinto de «la pestaña anterior» (`Ctrl+Tab`), que alterna entre las dos últimas: acá cada
 * paso queda en una pila, volver dos veces llega a la de hace dos, y avanzar deshace lo que se
 * retrocedió. Activar una pestaña nueva estando atrás corta lo que había adelante, que es lo que
 * hace cualquier historial. Las claves de pestañas ya cerradas se saltan al recorrer. Es puro y con
 * Vitest.
 */

const MAX = 50;

export class NavHistory {
  private entries: string[] = [];
  private index = -1;

  /** La que está a la vista según el historial. */
  get current(): string | null {
    return this.entries[this.index] ?? null;
  }

  get canGoBack(): boolean {
    return this.index > 0;
  }

  get canGoForward(): boolean {
    return this.index >= 0 && this.index < this.entries.length - 1;
  }

  /** Registra una pestaña recién activada. Repetir la actual no agrega nada. */
  push(key: string) {
    if (this.current === key) return;
    this.entries = this.entries.slice(0, this.index + 1);
    this.entries.push(key);
    if (this.entries.length > MAX) this.entries.shift();
    this.index = this.entries.length - 1;
  }

  /** Cierra una pestaña: sus entradas salen del historial, y las vecinas repetidas se juntan. */
  remove(key: string) {
    const current = this.current;
    const kept: string[] = [];
    for (const entry of this.entries) {
      if (entry !== key && kept.at(-1) !== entry) kept.push(entry);
    }
    this.entries = kept;
    const at = current === null || current === key ? -1 : kept.lastIndexOf(current);
    this.index = at === -1 ? kept.length - 1 : at;
  }

  /** Retrocede hasta una pestaña que siga abierta. `null` si no hay a dónde. */
  back(isOpen: (key: string) => boolean): string | null {
    for (let i = this.index - 1; i >= 0; i--) {
      if (isOpen(this.entries[i])) {
        this.index = i;
        return this.entries[i];
      }
    }
    return null;
  }

  forward(isOpen: (key: string) => boolean): string | null {
    for (let i = this.index + 1; i < this.entries.length; i++) {
      if (isOpen(this.entries[i])) {
        this.index = i;
        return this.entries[i];
      }
    }
    return null;
  }
}
