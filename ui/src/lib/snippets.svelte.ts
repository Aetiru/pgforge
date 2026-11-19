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
  snippetsExport,
  snippetsImport,
  snippetsList,
  snippetsReset,
  snippetUsage,
  snippetUsed,
  type ImportMode,
  type ImportReport,
  type Snippet,
  type SnippetUse,
} from "./ipc";
import { normalizeCombo } from "./snippet-check";
import { visibleFor as visibleSnippets } from "./snippet-list";

class Snippets {
  items = $state<Snippet[]>([]);
  /** Cuántas veces se expandió cada una, por id. Es del equipo: no viaja con las abreviaturas. */
  usage = $state<Record<string, SnippetUse>>({});
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
    // Los contadores son un adorno de la lista: si no se leen, ordenar por usos no ordena nada y
    // nada más se rompe.
    try {
      this.usage = await snippetUsage();
    } catch {
      this.usage = {};
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

  /**
   * Anota un uso. Es instantáneo a propósito: la expansión corre en la misma pulsación del tabulador
   * y no puede esperar un `invoke`, así que el contador local se actualiza ya y el guardado se manda
   * sin esperarlo ni mostrar su fallo (perder un uso no rompe nada).
   */
  noteUsed(id: string) {
    const current = this.usage[id];
    this.usage = {
      ...this.usage,
      [id]: { uses: (current?.uses ?? 0) + 1, lastUsedAt: Math.floor(Date.now() / 1000) },
    };
    void snippetUsed(id).catch(() => {});
  }

  /**
   * Borra y devuelve lo borrado, para que la interfaz ofrezca «Deshacer» durante unos segundos
   * (volver a guardarlo con `save` lo repone con el mismo id). `null` si no estaba o si falló.
   */
  async removeWithUndo(id: string): Promise<Snippet | null> {
    const removed = this.items.find((item) => item.id === id);
    if (!removed) return null;
    if (!(await this.remove(id))) return null;
    const { [id]: _gone, ...rest } = this.usage;
    this.usage = rest;
    return removed;
  }

  /** Exporta las elegidas (o todas con `null`) a un archivo; devuelve cuántas escribió. */
  async exportTo(path: string, ids: string[] | null): Promise<number | null> {
    try {
      const count = await snippetsExport(path, ids);
      this.error = null;
      return count;
    } catch (error) {
      this.error = describeError(error);
      return null;
    }
  }

  /** Importa de un archivo y toma la lista que devuelve el núcleo, que ya incluye lo importado. */
  async importFrom(path: string, mode: ImportMode): Promise<ImportReport | null> {
    try {
      const report = await snippetsImport(path, mode);
      this.items = report.snippets;
      this.error = null;
      return report;
    } catch (error) {
      this.error = describeError(error);
      return null;
    }
  }

  /** Las que valen en este servidor y esta base (ver `scopeMatches`). */
  visibleFor(profileId: string | null, database: string | null): Snippet[] {
    return visibleSnippets(this.items, profileId, database);
  }

  /** La abreviatura que tiene este atajo, si hay una; se compara normalizado (`Mod-j` = `Ctrl-j`). */
  byShortcut(combo: string): Snippet | undefined {
    const key = normalizeCombo(combo);
    if (key === null) return undefined;
    return this.items.find((item) => normalizeCombo(item.shortcut) === key);
  }
}

export const snippets = new Snippets();
