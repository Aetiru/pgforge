/**
 * Servidores a los que el usuario se conectó hace poco, para encontrarlos sin bajar por la carpeta
 * donde viven. Va en `localStorage`, igual que `folders.svelte.ts`: es una preferencia de esta
 * máquina, no algo que el núcleo tenga por qué saber.
 *
 * Se anota al conectar y no al seleccionar una fila: mirar una tabla no es "usar" el servidor, y
 * anotar cada clic llenaría la lista con lo que se pasó de largo camino a otra cosa.
 */

import { wGet, wSet } from "./wstorage";

const KEY = "pgforge.recents";

/** Cuántos se recuerdan. Más que esto deja de ser "reciente" y empieza a ser una copia del árbol. */
const MAX = 6;

function stored(): string[] {
  try {
    const value = JSON.parse(wGet(KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((id) => typeof id === "string") : [];
  } catch {
    return [];
  }
}

class Recents {
  private ids = $state<string[]>(stored());

  /** Del más reciente al menos, sin filtrar por si el perfil todavía existe: eso lo hace quien lee. */
  get list(): string[] {
    return this.ids;
  }

  /** El servidor pasa a ser el más reciente. Ya estar en la lista no lo duplica, lo adelanta. */
  touch(profileId: string) {
    this.ids = [profileId, ...this.ids.filter((id) => id !== profileId)].slice(0, MAX);
    wSet(KEY, JSON.stringify(this.ids));
  }

  /** Lo saca de la lista: el usuario lo descartó, o el perfil dejó de existir. */
  forget(profileId: string) {
    if (!this.ids.includes(profileId)) return;
    this.ids = this.ids.filter((id) => id !== profileId);
    wSet(KEY, JSON.stringify(this.ids));
  }
}

export const recents = new Recents();
