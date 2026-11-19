import { versionClear, versionSave } from "../ipc";
import { versionKey, VersionScheduler, type VersionedTab } from "./versions";

/** Una pestaña de consulta, vista desde acá: su texto además de lo que da la clave. */
export type VersionableTab = VersionedTab & { sql: string };

/**
 * La red de seguridad del texto: copias de lo que se escribió, más allá de «deshacer», que se pierde
 * al cerrar la pestaña. Vive fuera de los componentes porque las copias se guardan **aunque** la
 * pestaña no esté a la vista (el panel se destruye al cambiar de pestaña; el temporizador no).
 *
 * Nada de esto muestra errores ni espera a nadie: ver `VersionScheduler`.
 */
class Versions {
  private readonly scheduler = new VersionScheduler((key, text) => versionSave(key, text));

  key(tab: VersionedTab): string {
    return versionKey(tab);
  }

  /** El texto cambió: se guarda una copia si pasan 90 s sin teclear. */
  touch(tab: VersionableTab) {
    this.scheduler.touch(versionKey(tab), () => tab.sql);
  }

  /** Antes de ejecutar: lo que se está por correr es lo que uno querría poder recuperar. */
  noteRun(tab: VersionableTab) {
    this.scheduler.saveNow(versionKey(tab), tab.sql);
  }

  /** Antes de reemplazar el texto (restaurar una copia): para que el reemplazo también se deshaga. */
  snapshot(tab: VersionableTab) {
    this.scheduler.saveNow(versionKey(tab), tab.sql);
  }

  noteClose(tab: VersionableTab) {
    this.scheduler.close(versionKey(tab), tab.sql);
  }

  async clear(tab: VersionedTab) {
    const key = versionKey(tab);
    await versionClear(key);
    this.scheduler.forget(key);
  }
}

export const versions = new Versions();
