/**
 * Las abreviaturas del editor, del lado de la interfaz.
 *
 * A diferencia del tamaño de letra o del alto del editor, que viven en `localStorage`, esto es un
 * espejo de lo que guarda Rust en `snippets.json`: son del usuario y se respaldan con el resto de su
 * configuración (ver `sql::snippet`). Acá solo se cachean para que el atajo del tabulador las tenga
 * en el acto —tiene que responder en la misma pulsación, no puede esperar un `invoke`—.
 *
 * Se cargan una sola vez al arrancar. Cada guardado devuelve la lista entera, así que la copia local
 * se repone con lo que contestó el servidor y no con lo que la pantalla creía tener.
 */

import {
  describeError,
  snippetDelete,
  snippetSave,
  snippetsList,
  snippetsReset,
  type Snippet,
} from "./ipc";

class Snippets {
  items = $state<Snippet[]>([]);
  /** Lo último que falló, para que el diálogo lo muestre. */
  error = $state<string | null>(null);
  private started = false;

  /**
   * Trae la lista si todavía no se trajo.
   *
   * Que falle **no se muestra**: la aplicación arranca igual y sin abreviaturas, que es peor que
   * tenerlas pero mucho mejor que un cartel rojo al abrir. El diálogo sí informa sus propios fallos,
   * porque ahí el usuario está esperando una respuesta.
   */
  async load() {
    if (this.started) return;
    this.started = true;
    try {
      this.items = await snippetsList();
    } catch {
      this.items = [];
    }
  }

  private async apply(run: () => Promise<Snippet[]>): Promise<boolean> {
    try {
      this.items = await run();
      this.error = null;
      return true;
    } catch (error) {
      // Por el embudo de siempre: es el que además lo anota en el registro.
      this.error = describeError(error);
      return false;
    }
  }

  save(snippet: Snippet) {
    return this.apply(() => snippetSave(snippet));
  }

  remove(id: string) {
    return this.apply(() => snippetDelete(id));
  }

  reset() {
    return this.apply(() => snippetsReset());
  }
}

export const snippets = new Snippets();
