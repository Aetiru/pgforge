/**
 * El árbol de scripts, del lado de la interfaz: los `.sql` de cada conexión, guardados solos en
 * `<config>/scripts/<conexión>/`.
 *
 * Qué carpetas quedaron colapsadas vive en `localStorage` con el patrón **exacto** de
 * `folders.svelte.ts`: se guarda lo colapsado y no lo abierto, así que una carpeta nueva —o un
 * script recién importado en una subcarpeta que no existía— aparece abierta sin que nadie lo pida.
 */

import { describeError, scriptsTree, type ScriptFolder } from "./ipc";
import { wGet, wSet } from "./wstorage";

const COLLAPSED_KEY = "pgforge.library.scriptsCollapsed";

function stored(): string[] {
  try {
    const value = JSON.parse(wGet(COLLAPSED_KEY) ?? "[]");
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

  /** Carpetas que el usuario dejó cerradas, identificadas por su ruta completa. */
  private collapsed = $state<string[]>(stored());

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

  /** Si la carpeta tiene que dibujarse abierta. Una carpeta nueva arranca abierta. */
  isOpen(path: string): boolean {
    return !this.collapsed.includes(path);
  }

  setOpen(path: string, open: boolean) {
    const without = this.collapsed.filter((item) => item !== path);
    this.collapsed = open ? without : [...without, path];
    wSet(COLLAPSED_KEY, JSON.stringify(this.collapsed));
  }
}

export const scripts = new Scripts();
