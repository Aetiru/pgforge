import { wGet, wSet } from "./wstorage";

/**
 * Servidores fijados arriba del árbol.
 *
 * Es una preferencia de esta ventana, no del perfil: no hay que migrar `connections.json` y fijar
 * no cambia nada de cómo se conecta. Lo que ya no existe se ignora al leer (ver `TreePanel`), así
 * que borrar un servidor no deja basura que limpiar.
 */
const KEY = "pgforge.pins";

export function parsePins(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const data: unknown = JSON.parse(raw);
    return Array.isArray(data) ? data.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

class Pins {
  ids = $state<string[]>(parsePins(wGet(KEY)));

  has(profileId: string): boolean {
    return this.ids.includes(profileId);
  }

  toggle(profileId: string) {
    this.ids = this.has(profileId)
      ? this.ids.filter((id) => id !== profileId)
      : [...this.ids, profileId];
    try {
      wSet(KEY, JSON.stringify(this.ids));
    } catch {
      // Fijar es una comodidad: sin almacenamiento, vale hasta cerrar la ventana.
    }
  }
}

export const pins = new Pins();
