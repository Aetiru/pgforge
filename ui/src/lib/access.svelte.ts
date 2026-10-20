import { explorer } from "./explorer.svelte";
import type { ConnectionProfile, Environment } from "./ipc";
import { assessRisks, type StatementRisk } from "./statement-risk";

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

/**
 * El SQL que se va a ejecutar, para decidir cuánta confirmación pide. Puede ser una función porque
 * armarlo cuesta una ida al núcleo (la vista previa del diálogo) y fuera de producción no hace falta.
 */
export type SqlSource = string | null | undefined | (() => Promise<string | null | undefined>);

interface PendingConfirm {
  profile: ConnectionProfile;
  action: string;
  /**
   * Sentencias que piden la confirmación fuerte. Con alguna, `App.svelte` exige escribir el nombre
   * del servidor para habilitar el botón.
   */
  risks: StatementRisk[];
  resolve: (allowed: boolean) => void;
}

async function sqlOf(source: SqlSource): Promise<string | null> {
  try {
    return (typeof source === "function" ? await source() : source) ?? null;
  } catch {
    // Si no se pudo armar el SQL, quien lo pidió va a fallar al aplicarlo con un error mejor que
    // el que daríamos acá; la confirmación de siempre sigue valiendo.
    return null;
  }
}

/**
 * Confirmación extra antes de modificar un servidor de producción.
 *
 * La pregunta la dibuja `App.svelte` una sola vez y se resuelve por promesa, así ningún diálogo de
 * mutación tiene que anidar otro modal adentro del suyo.
 *
 * Con `DROP`, `TRUNCATE` o un `DELETE`/`UPDATE` sin `WHERE` en el SQL (ver `statement-risk.ts`) la
 * pregunta es la misma pero exige escribir el nombre del servidor: es el escalón que separa «sí,
 * quiero tocar producción» de «sí, quiero tirar esto».
 */
class MutationGuard {
  pending = $state<PendingConfirm | null>(null);

  /** Resuelve `true` si se puede seguir. Fuera de producción no pregunta nada. */
  async confirm(profileId: string, action: string, sql?: SqlSource): Promise<boolean> {
    const profile = profileOf(profileId);
    if (!profile || profile.environment !== "prod") return true;

    const text = await sqlOf(sql);
    return this.ask(profile, action, text ? assessRisks(text) : []);
  }

  /**
   * Para el editor de SQL, que hoy no pregunta nada: aquí solo se interrumpe cuando el script
   * trae algo de la lista, y el resto corre como siempre.
   */
  async confirmScript(profileId: string, sql: string): Promise<boolean> {
    const profile = profileOf(profileId);
    if (!profile || profile.environment !== "prod") return true;

    const risks = assessRisks(sql);
    if (risks.length === 0) return true;
    return this.ask(profile, "El script que vas a ejecutar incluye:", risks);
  }

  private ask(profile: ConnectionProfile, action: string, risks: StatementRisk[]) {
    return new Promise<boolean>((resolve) => {
      this.pending = { profile, action, risks, resolve };
    });
  }

  answer(allowed: boolean) {
    this.pending?.resolve(allowed);
    this.pending = null;
  }
}

export const guard = new MutationGuard();

/** Atajo con el nombre que usan los diálogos. */
export const confirmMutation = (profileId: string, action: string, sql?: SqlSource) =>
  guard.confirm(profileId, action, sql);

/** Para el editor: solo pregunta si el script trae algo destructivo. */
export const confirmScript = (profileId: string, sql: string) => guard.confirmScript(profileId, sql);
