import { describeError } from "../ipc";

/**
 * Lo que cada panel de diagnóstico repite: leer, guardar el error aparte del dato, y descartar la
 * respuesta de una lectura que ya no es la última (cambiar de base a mitad de la carga no tiene que
 * pintar las tablas de la base anterior).
 *
 * El dato anterior se conserva mientras carga la siguiente lectura: una lista que se vacía en cada
 * «Actualizar» parpadea y pierde el lugar donde uno estaba mirando.
 */
export class Loader<T> {
  data = $state.raw<T | null>(null);
  error = $state<string | null>(null);
  loading = $state(false);
  loadedAt = $state<number | null>(null);
  private run = 0;

  async load(read: () => Promise<T>) {
    const mine = ++this.run;
    this.loading = true;
    try {
      const result = await read();
      if (mine !== this.run) return;
      this.data = result;
      this.error = null;
      this.loadedAt = Date.now();
    } catch (error) {
      if (mine !== this.run) return;
      this.error = describeError(error);
    } finally {
      if (mine === this.run) this.loading = false;
    }
  }

  /** Suelta lo leído: es de otra base. */
  reset() {
    this.run += 1;
    this.data = null;
    this.error = null;
    this.loading = false;
    this.loadedAt = null;
  }
}
