/**
 * El aviso del sistema cuando termina algo que tardó.
 *
 * Los procesos largos se sacaron de sus diálogos justo para poder irse a hacer otra cosa mientras
 * corren, y una consulta pesada es lo mismo: quien la lanza se va a leer otra pantalla. Enterarse de
 * que terminó no puede depender de acordarse de volver a mirar, y menos con la ventana detrás de
 * otra o minimizada, que es exactamente cuando conviene avisar.
 *
 * Dos decisiones que sostienen esto:
 *
 * - **Se avisa igual con la ventana adelante.** Filtrar por foco parece más prolijo y no lo es: la
 *   ventana puede estar visible en otro monitor, o adelante pero con la vista de procesos cerrada.
 *   El aviso del sistema se descarta solo; el que no llega no vuelve.
 * - **Pero no de lo que terminó enseguida.** Un `SELECT 1` que tarda veinte milisegundos no
 *   interrumpe a nadie, y una notificación por cada ejecución convertiría el aviso en ruido que se
 *   apaga entero. Por debajo del umbral no se avisa: el resultado ya está en pantalla, que es donde
 *   estaba mirando quien lo lanzó.
 *
 * El permiso se pide una sola vez, la primera vez que hay algo para avisar: pedirlo al arrancar
 * sería un cartel del sistema antes de que el usuario hiciera nada.
 */

import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import { taskKindLabel } from "./task-format";
import type { TaskRun } from "./tasks.svelte";
import { duration } from "./format";

// Preferencias de toda la aplicación, no de esta ventana: si se apagan los avisos o se cambia el
// umbral, tiene que valer para todas las ventanas abiertas (mismo criterio que `update.svelte.ts`,
// `theme.svelte.ts` y `paging.svelte.ts`), así que van a `localStorage` directo y no a `wGet`/`wSet`.
const ENABLED_KEY = "pgforge.notify.enabled";
const SECONDS_KEY = "pgforge.notify.seconds";

/**
 * Identificadores de proceso ya notificados por el sistema operativo, **compartida entre todas las
 * ventanas** (`localStorage` directo, no `wGet`/`wSet`): el registro de procesos es único del lado
 * de Rust y llega por igual a cada ventana abierta (ver `process_watch`), así que sin esta lista un
 * mismo `VACUUM` que termina con tres ventanas abiertas dispara tres banners nativos para el mismo
 * evento. Mismo orden de magnitud que `KEEP_FINISHED` de `process.rs`.
 */
const NOTIFIED_KEY = "pgforge.notify.taskIds";
const NOTIFIED_LIMIT = 50;

function notifiedIds(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(NOTIFIED_KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((id) => typeof id === "string") : [];
  } catch {
    return [];
  }
}

/**
 * `true` la primera vez que se ve este `taskId`, y lo anota. Las ventanas no se coordinan entre
 * sí —no hay lock ni mensaje cruzado—, así que dos ventanas que lean `localStorage` en el mismo
 * instante pueden las dos creer que son la primera: es una carrera rarísima y el costo de un aviso
 * nativo duplicado ocasional es menor que el de sincronizar esto de verdad.
 */
function claimNotification(taskId: string): boolean {
  const ids = notifiedIds();
  if (ids.includes(taskId)) return false;
  try {
    localStorage.setItem(NOTIFIED_KEY, JSON.stringify([...ids, taskId].slice(-NOTIFIED_LIMIT)));
  } catch {
    // Nada que hacer sin `localStorage`: en el peor caso, se repite este aviso más adelante.
  }
  return true;
}

/**
 * A partir de cuántos segundos se avisa.
 *
 * Diez es el punto donde uno ya se fue a otra cosa: por debajo, el resultado aparece mientras
 * todavía se está mirando la pantalla que lo pidió.
 */
export const DEFAULT_MIN_SECONDS = 10;

export const MIN_SECONDS_CHOICES = [0, 5, 10, 30, 60] as const;

function storedEnabled(): boolean {
  try {
    return localStorage.getItem(ENABLED_KEY) !== "off";
  } catch {
    // Ni un valor corrupto ni la falta de `localStorage` —los tests corren en Node— pueden impedir
    // que esto se cargue.
    return true;
  }
}

function storedSeconds(): number {
  try {
    const value = Number(localStorage.getItem(SECONDS_KEY));
    return (MIN_SECONDS_CHOICES as readonly number[]).includes(value) ? value : DEFAULT_MIN_SECONDS;
  } catch {
    return DEFAULT_MIN_SECONDS;
  }
}

class Notify {
  enabled = $state(storedEnabled());
  /** Cero avisa de todo, que es lo que pide quien deja la aplicación de fondo todo el día. */
  minSeconds = $state(storedSeconds());

  /** `null` mientras no se preguntó todavía. */
  private granted: boolean | null = null;

  setEnabled(on: boolean) {
    this.enabled = on;
    try {
      localStorage.setItem(ENABLED_KEY, on ? "on" : "off");
    } catch {
      // Nada que hacer sin `localStorage`: la preferencia no se recuerda esta vez.
    }
  }

  setMinSeconds(seconds: number) {
    this.minSeconds = seconds;
    try {
      localStorage.setItem(SECONDS_KEY, String(seconds));
    } catch {
      // Nada que hacer sin `localStorage`: la preferencia no se recuerda esta vez.
    }
  }

  /**
   * Un proceso de la vista de procesos que terminó, bien o mal.
   *
   * El registro de procesos es de Rust y llega igual a cada ventana abierta: sin `claimNotification`
   * acá, el mismo evento dispararía un banner nativo por ventana. El toast en pantalla no pasa por
   * acá —cada ventana muestra el suyo, y eso sí es lo esperado—.
   */
  async taskEnded(run: TaskRun) {
    const seconds = ((run.finishedAt ?? Date.now()) - run.startedAt) / 1000;
    const what = `${taskKindLabel(run.kind)} · ${run.target}`;

    await this.send(
      seconds,
      run.status === "failed" ? `Falló: ${what}` : `Terminó: ${what}`,
      `${run.server}${run.database ? ` / ${run.database}` : ""} · ${duration(seconds)}`,
      run.taskId,
    );
  }

  /** Una ejecución de la pestaña de consulta. */
  async queryEnded(init: { server: string; database: string; seconds: number; failed: boolean }) {
    await this.send(
      init.seconds,
      init.failed ? "La consulta falló" : "La consulta terminó",
      `${init.server}${init.database ? ` / ${init.database}` : ""} · ${duration(init.seconds)}`,
    );
  }

  /**
   * `taskId` solo lo trae `taskEnded`: una consulta no cruza el registro de procesos de Rust, así
   * que no puede duplicarse entre ventanas de la misma forma. El `claimNotification` va **acá**, ya
   * decidido que esta ventana iba a avisar —después de `enabled` y del umbral—, para no gastar el
   * turno compartido con una ventana que igual no iba a notificar.
   */
  private async send(seconds: number, title: string, body: string, taskId?: string) {
    if (!this.enabled || seconds < this.minSeconds) return;
    if (taskId && !claimNotification(taskId)) return;

    try {
      if (this.granted === null) {
        this.granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
      }
      if (this.granted) sendNotification({ title, body });
    } catch {
      // Que el sistema no deje avisar no es una falla de lo que se estaba haciendo: eso ya terminó
      // y su resultado está en la vista de procesos. Un cartel de error por un aviso sería peor que
      // el aviso que no llegó.
    }
  }
}

export const notify = new Notify();
