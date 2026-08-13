/**
 * Lo compartido por toda la frontera: el error del núcleo y los ayudantes que no pertenecen a
 * ningún dominio. Los demás módulos lo importan de acá.
 */

import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import { error as logError } from "@tauri-apps/plugin-log";

/** Error del núcleo tal como cruza el IPC. Refleja `pgforge_core::error::ErrorPayload`. */
export type CoreError =
  | { kind: "canceled" }
  /** Los datos cambiaron entre que se leyeron y se quisieron escribir. */
  | { kind: "conflict"; message: string }
  | { kind: "permission"; message: string }
  | {
      kind: "database";
      code: string;
      message: string;
      detail: string | null;
      hint: string | null;
      position: number | null;
    }
  /** La clave del host SSH no está en `known_hosts`, o cambió. La interfaz muestra la huella. */
  | { kind: "sshHostKey"; host: string; fingerprint: string; changed: boolean }
  /** El servidor no está del otro lado: se cayó, lo reiniciaron o se cortó la red. */
  | { kind: "disconnected"; message: string }
  | { kind: "other"; message: string };

export { Channel };

/**
 * Quién se entera de que un servidor dejó de responder.
 *
 * Un corte no es un error de la operación que se estaba haciendo: es del vínculo, y cualquier otra
 * cosa contra ese servidor va a fallar igual. Enterarse en cada sitio que llama —y hay decenas—
 * sería repetir el mismo `catch` en toda la aplicación, así que el aviso sale de un solo lugar: el
 * `invoke` de acá abajo, que ve pasar todas las llamadas y todos los errores.
 */
type ServerDownHandler = (profileId: string) => void;

let serverDown: ServerDownHandler | null = null;

export function onServerDown(handler: ServerDownHandler) {
  serverDown = handler;
}

/**
 * `invoke` de Tauri, con el corte de conexión avisado de paso.
 *
 * Todos los módulos de `ipc/` llaman a este y no al de Tauri. El identificador del servidor sale
 * del propio argumento `id`, que es el que llevan casi todos los comandos; los pocos que no lo
 * llevan —los de vista previa, que son puros— tampoco pueden cortarse por una conexión caída.
 */
export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    if ((error as CoreError)?.kind === "disconnected" && typeof args?.id === "string") {
      serverDown?.(args.id);
    }
    throw error;
  }
}

/** `true` si el error dice que el servidor dejó de estar del otro lado. */
export function isDisconnected(error: unknown): boolean {
  return (error as CoreError)?.kind === "disconnected";
}

/** `160004` se muestra como `16.4`. */
export function formatVersion(versionNum: number): string {
  return `${Math.floor(versionNum / 10000)}.${versionNum % 10000}`;
}

/** `true` si el error viene de una cancelación pedida por el usuario, que no es una falla. */
export function isCanceled(error: unknown): boolean {
  return typeof error === "object" && error !== null && (error as CoreError).kind === "canceled";
}

/**
 * Texto legible para cualquier error del núcleo.
 *
 * De paso lo escribe en el registro, porque es el **único embudo** por el que pasa todo error que
 * el usuario llega a ver: un error que se muestra en un cartel y se cierra no deja rastro, y
 * después «me falló al conectar» no se puede diagnosticar. Una cancelación no se registra: no es
 * una falla, la pidió el usuario.
 *
 * El registro va sin esperar y con el fallo tragado a propósito: fuera de la ventana de Tauri —los
 * tests de Vitest— no hay a quién invocar, y no poder anotar un error no puede convertirse en otro.
 */
export function describeError(error: unknown): string {
  const message = errorText(error);
  if ((error as CoreError)?.kind !== "canceled") {
    void logError(message).catch(() => {});
  }
  return message;
}

function errorText(error: unknown): string {
  const e = error as CoreError;
  switch (e?.kind) {
    case "canceled":
      return "Operación cancelada.";
    case "conflict":
      return e.message;
    case "permission":
      return `Permiso insuficiente: ${e.message}`;
    case "database":
      return e.hint ? `${e.message} (${e.hint})` : e.message;
    case "sshHostKey":
      return e.changed
        ? `La clave del host SSH ${e.host} cambió (huella ${e.fingerprint}). Podría ser un intermediario.`
        : `El host SSH ${e.host} no está verificado (huella ${e.fingerprint}).`;
    case "disconnected":
      return `Se perdió la conexión con el servidor: ${e.message}`;
    case "other":
      return e.message;
    default:
      return String(error);
  }
}

/** Devuelve el error de clave de host SSH sin verificar, o `null` si el error es de otro tipo. */
export function sshHostKey(
  error: unknown,
): { host: string; fingerprint: string; changed: boolean } | null {
  const e = error as CoreError;
  return e?.kind === "sshHostKey"
    ? { host: e.host, fingerprint: e.fingerprint, changed: e.changed }
    : null;
}
