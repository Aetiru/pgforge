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

const ENABLED_KEY = "pgforge.notify.enabled";
const SECONDS_KEY = "pgforge.notify.seconds";

/**
 * A partir de cuántos segundos se avisa.
 *
 * Diez es el punto donde uno ya se fue a otra cosa: por debajo, el resultado aparece mientras
 * todavía se está mirando la pantalla que lo pidió.
 */
export const DEFAULT_MIN_SECONDS = 10;

export const MIN_SECONDS_CHOICES = [0, 5, 10, 30, 60] as const;

function storedEnabled(): boolean {
  return localStorage.getItem(ENABLED_KEY) !== "off";
}

function storedSeconds(): number {
  const value = Number(localStorage.getItem(SECONDS_KEY));
  return (MIN_SECONDS_CHOICES as readonly number[]).includes(value) ? value : DEFAULT_MIN_SECONDS;
}

class Notify {
  enabled = $state(storedEnabled());
  /** Cero avisa de todo, que es lo que pide quien deja la aplicación de fondo todo el día. */
  minSeconds = $state(storedSeconds());

  /** `null` mientras no se preguntó todavía. */
  private granted: boolean | null = null;

  setEnabled(on: boolean) {
    this.enabled = on;
    localStorage.setItem(ENABLED_KEY, on ? "on" : "off");
  }

  setMinSeconds(seconds: number) {
    this.minSeconds = seconds;
    localStorage.setItem(SECONDS_KEY, String(seconds));
  }

  /** Un proceso de la vista de procesos que terminó, bien o mal. */
  async taskEnded(run: TaskRun) {
    const seconds = ((run.finishedAt ?? Date.now()) - run.startedAt) / 1000;
    const what = `${taskKindLabel(run.kind)} · ${run.target}`;

    await this.send(
      seconds,
      run.status === "failed" ? `Falló: ${what}` : `Terminó: ${what}`,
      `${run.server}${run.database ? ` / ${run.database}` : ""} · ${duration(seconds)}`,
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

  private async send(seconds: number, title: string, body: string) {
    if (!this.enabled || seconds < this.minSeconds) return;

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
