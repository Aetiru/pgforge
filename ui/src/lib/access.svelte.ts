import { explorer } from "./explorer.svelte";
import type { ConnectionProfile, Environment } from "./ipc";

/**
 * Lo que el perfil de un servidor habilita o exige antes de modificar algo.
 *
 * Vive aparte de `explorer` porque lo consultan los diálogos de mutación, que no tienen nada que ver
 * con el árbol: sin este módulo, cada uno tendría que importarse el explorador entero para preguntar
 * si el servidor es de solo lectura.
 */
function profileOf(profileId: string): ConnectionProfile | null {
  return explorer.profiles.find((profile) => profile.id === profileId) ?? null;
}

export function environmentOf(profileId: string): Environment | null {
  return profileOf(profileId)?.environment ?? null;
}

export function isReadOnly(profileId: string): boolean {
  return profileOf(profileId)?.readOnly ?? false;
}

/**
 * Por qué no se puede escribir, para poner en el `title` de un botón deshabilitado. `null` cuando sí
 * se puede: un botón apagado sin explicación es peor que no tenerlo.
 */
export function readOnlyReason(profileId: string): string | null {
  return isReadOnly(profileId)
    ? "El servidor está configurado como conexión de solo lectura"
    : null;
}

interface PendingConfirm {
  profile: ConnectionProfile;
  action: string;
  resolve: (allowed: boolean) => void;
}

/**
 * Confirmación extra antes de modificar un servidor de producción.
 *
 * La pregunta la dibuja `App.svelte` una sola vez y se resuelve por promesa, así ningún diálogo de
 * mutación tiene que anidar otro modal adentro del suyo.
 */
class MutationGuard {
  pending = $state<PendingConfirm | null>(null);

  /** Resuelve `true` si se puede seguir. Fuera de producción no pregunta nada. */
  confirm(profileId: string, action: string): Promise<boolean> {
    const profile = profileOf(profileId);
    if (!profile || profile.environment !== "prod") return Promise.resolve(true);

    return new Promise((resolve) => {
      this.pending = { profile, action, resolve };
    });
  }

  answer(allowed: boolean) {
    this.pending?.resolve(allowed);
    this.pending = null;
  }
}

export const guard = new MutationGuard();

/** Atajo con el nombre que usan los diálogos. */
export const confirmMutation = (profileId: string, action: string) =>
  guard.confirm(profileId, action);
