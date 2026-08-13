/**
 * Lo compartido por toda la frontera: el error del núcleo y los ayudantes que no pertenecen a
 * ningún dominio. Los demás módulos lo importan de acá.
 */

import { Channel } from "@tauri-apps/api/core";
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
  | { kind: "other"; message: string };

export { Channel };

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
