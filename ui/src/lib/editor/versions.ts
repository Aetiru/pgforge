/**
 * Versiones anteriores del texto de una pestaña de consulta: lo que decide cuándo guardar una copia
 * y cómo identificar la pestaña. Sin DOM ni ipc, para poder probarlo con un reloj inyectado.
 */


import { t } from "../i18n.svelte";
/** Lo único que hace falta saber de una pestaña para ponerle clave. */
export interface VersionedTab {
  key: string;
  profileId: string;
  scriptPath: string | null;
  filePath: string | null;
  savedId: number | null;
}

/**
 * La clave con la que el núcleo agrupa las copias de una pestaña.
 *
 * El identificador local de la pestaña (`tab-7`) **no** sobrevive a un reinicio: se vuelve a contar
 * desde cero. Por eso se prefiere lo que sí sobrevive —el script del árbol, el archivo `.sql` o la
 * consulta guardada—, y solo una pestaña sin nada de eso queda atada a su identificador local.
 * Límite conocido: una pestaña sin script ni archivo restaurada tras reiniciar es otra pestaña para
 * este historial, y renombrar un script cambia su `scriptPath` (las copias previas quedan con la
 * clave vieja).
 */
export function versionKey(tab: VersionedTab): string {
  if (tab.scriptPath) return `script:${tab.profileId}:${tab.scriptPath}`;
  if (tab.filePath) return `file:${tab.filePath}`;
  if (tab.savedId !== null) return `saved:${tab.savedId}`;
  return `tab:${tab.profileId}:${tab.key}`;
}

/** Texto vacío o igual al último guardado: no vale el viaje al núcleo. */
export function worthSaving(text: string, lastSaved: string | null | undefined): boolean {
  return text.trim() !== "" && text !== lastSaved;
}

export interface Timers {
  set: (callback: () => void, ms: number) => unknown;
  clear: (handle: unknown) => void;
}

const REAL_TIMERS: Timers = {
  set: (callback, ms) => setTimeout(callback, ms),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/** Segundos de quietud tras los que se guarda una copia, si el texto cambió. */
export const IDLE_SECONDS = 90;

/**
 * La política: guarda al ejecutar, tras `IDLE_SECONDS` sin teclear y al cerrar. Todo es silencioso:
 * una red de seguridad que muestra errores o bloquea deja de ser una red.
 */
export class VersionScheduler {
  private readonly last = new Map<string, string>();
  private readonly timers = new Map<string, unknown>();

  constructor(
    private readonly save: (key: string, text: string) => Promise<unknown>,
    private readonly idleMs = IDLE_SECONDS * 1000,
    private readonly clock: Timers = REAL_TIMERS,
  ) {}

  /** Cada cambio del texto reinicia la cuenta de quietud. `read` se llama recién al vencer. */
  touch(key: string, read: () => string) {
    this.cancel(key);
    this.timers.set(
      key,
      this.clock.set(() => {
        this.timers.delete(key);
        this.saveNow(key, read());
      }, this.idleMs),
    );
  }

  /** Guarda ya, si vale la pena. Devuelve si lo intentó. */
  saveNow(key: string, text: string): boolean {
    if (!worthSaving(text, this.last.get(key))) return false;
    this.last.set(key, text);
    // Si falla el guardado se olvida el «último», para reintentar con el mismo texto la próxima vez.
    this.save(key, text).catch(() => {
      if (this.last.get(key) === text) this.last.delete(key);
    });
    return true;
  }

  /** Al cerrar: guarda lo pendiente y suelta el temporizador y la memoria de la pestaña. */
  close(key: string, text: string) {
    this.cancel(key);
    this.saveNow(key, text);
    this.last.delete(key);
  }

  /** Después de borrar las copias hay que volver a poder guardar el mismo texto. */
  forget(key: string) {
    this.last.delete(key);
  }

  private cancel(key: string) {
    const handle = this.timers.get(key);
    if (handle !== undefined) this.clock.clear(handle);
    this.timers.delete(key);
  }
}

/** «hace 20 min», con la escala que importa para decidir a cuál volver. */
export function relativeTime(at: number, now: number): string {
  const seconds = Math.max(0, now - at);
  if (seconds < 60) return t("hace un momento");
  if (seconds < 3600) return t("hace {floor} min", { floor: Math.floor(seconds / 60) });
  if (seconds < 86_400) {
    const hours = Math.floor(seconds / 3600);
    return t("hace {hours} h", { hours });
  }
  const days = Math.floor(seconds / 86_400);
  return days === 1 ? t("ayer") : t("hace {days} días", { days });
}
