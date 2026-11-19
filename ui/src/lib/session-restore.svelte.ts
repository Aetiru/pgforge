import { openQuery, QueryTab } from "./query.svelte";
import { tabs } from "./tabs.svelte";
import { MAX_TABS, parseSaved, snapshotOf, type SavedTab } from "./session-snapshot";
import { wGet, wSet } from "./wstorage";

/**
 * Recordar las consultas abiertas entre ejecuciones.
 *
 * Una pestaña vive sobre una conexión que no sobrevive al cierre, así que lo que se guarda no es la
 * pestaña sino lo que se perdería: el texto, el título, de qué servidor y base era y los archivos
 * a los que apunta. Se vuelve a abrir cuando ese servidor se conecta de nuevo, no antes: abrirla
 * sin conexión pediría contraseñas fuera del único lugar donde se piden.
 *
 * Solo consultas con texto: una pestaña vacía no es trabajo que perder, y los datos, el diagrama o
 * el dashboard se reabren con un clic desde el árbol.
 */
const KEY = "pgforge.session.tabs";

class SessionRestore {
  /** Lo que había abierto la vez anterior y todavía no se reabrió. */
  pending = $state<SavedTab[]>(parseSaved(wGet(KEY)));
  private timer: ReturnType<typeof setTimeout> | undefined;

  /** Cuántas hay esperando a un servidor, para avisarlo en la interfaz. */
  countFor(profileId: string): number {
    return this.pending.filter((item) => item.profileId === profileId).length;
  }

  /**
   * Escribe lo abierto más lo pendiente: sin sumar lo pendiente, lo primero que escribiera el efecto
   * —con cero pestañas abiertas— borraría todo lo que aún espera su servidor.
   */
  save() {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.flush(), 500);
  }

  flush() {
    clearTimeout(this.timer);
    const open = tabs.all
      .filter((tab): tab is QueryTab => tab instanceof QueryTab && tab.sql.trim() !== "")
      .map(snapshotOf);
    try {
      wSet(KEY, JSON.stringify([...open, ...this.pending].slice(0, MAX_TABS)));
    } catch {
      // Sin espacio o sin almacenamiento: recordar las pestañas es una comodidad, no puede fallar.
    }
  }

  /** Reabre lo que esperaba a este servidor. Cada entrada sale de la lista antes de abrirse. */
  async restoreFor(profileId: string): Promise<number> {
    const mine = this.pending.filter((item) => item.profileId === profileId);
    if (mine.length === 0) return 0;
    this.pending = this.pending.filter((item) => item.profileId !== profileId);

    let last: QueryTab | null = null;
    for (const saved of mine) {
      const tab = await openQuery(saved.profileId, saved.database, saved.title, false);
      tab.sql = saved.sql;
      tab.scriptPath = saved.scriptPath;
      // El script ya existía en disco si la pestaña tenía texto: no se crea otro vacío al abrir.
      tab.scriptCreated = saved.scriptPath !== null;
      tab.filePath = saved.filePath;
      tab.savedId = saved.savedId;
      tab.savedName = saved.savedName;
      last = tab;
    }
    if (last) tabs.activate(last.key);
    this.flush();
    return mine.length;
  }

  /** Descarta lo que esperaba a servidores que ya no existen. */
  prune(knownProfileIds: Set<string>) {
    const kept = this.pending.filter((item) => knownProfileIds.has(item.profileId));
    if (kept.length !== this.pending.length) this.pending = kept;
  }
}

export const session = new SessionRestore();
