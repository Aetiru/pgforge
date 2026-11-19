/**
 * El árbol de scripts, del lado de la interfaz: los `.sql` de cada conexión, guardados solos en
 * `<config>/scripts/<conexión>/`.
 *
 * Qué carpetas están abiertas vive en `localStorage`. Se guarda lo **abierto** y no lo colapsado:
 * con una carpeta por conexión y decenas de scripts cada una, que todo arrancara desplegado
 * convertía la lista en un muro; una carpeta nueva aparece cerrada y se abre cuando se la busca.
 */

import { describeError, scriptsTree, type ScriptFolder } from "./ipc";
import { wGet, wSet } from "./wstorage";

// Clave nueva: la anterior guardaba lo colapsado, y leerla al revés abriría justo lo cerrado.
const OPEN_KEY = "pgforge.library.scriptsOpen";

function stored(): string[] {
  try {
    const value = JSON.parse(wGet(OPEN_KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((name) => typeof name === "string") : [];
  } catch {
    // Un valor corrupto no puede impedir que esto se cargue.
    return [];
  }
}

class Scripts {
  tree = $state<ScriptFolder[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  /** Carpetas que el usuario abrió, identificadas por su ruta completa. */
  private opened = $state<string[]>(stored());

  /**
   * Relee el árbol entero. A diferencia de `Bookmarks.load`, no se guarda una sola vez: renombrar,
   * borrar, crear una carpeta o importar cambian el disco por fuera de este estado, y cada una de
   * esas acciones vuelve a llamarla.
   */
  async load() {
    this.loading = true;
    this.error = null;
    try {
      this.tree = await scriptsTree();
    } catch (failure) {
      this.error = describeError(failure);
    } finally {
      this.loading = false;
    }
  }

  /** Si la carpeta tiene que dibujarse abierta. Una carpeta nueva arranca cerrada. */
  isOpen(path: string): boolean {
    return this.opened.includes(path);
  }

  setOpen(path: string, open: boolean) {
    const without = this.opened.filter((item) => item !== path);
    this.opened = open ? [...without, path] : without;
    wSet(OPEN_KEY, JSON.stringify(this.opened));
  }
}

export const scripts = new Scripts();
