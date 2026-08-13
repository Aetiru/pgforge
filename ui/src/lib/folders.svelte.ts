/**
 * Lo que la aplicación recuerda de las carpetas de conexiones entre una sesión y la siguiente:
 * cuáles estaban cerradas y cuáles quedaron vacías.
 *
 * Va en `localStorage` y no en `connections.json` a propósito. Una carpeta no es una entidad
 * guardada: es un nombre que comparten unos perfiles (`ProfileStore::groups()` las deriva), así que
 * una carpeta vacía no tiene dónde persistir del lado de Rust sin inventarle una lista aparte y una
 * migración al archivo de conexiones. Y si está abierta o cerrada es preferencia de esta máquina,
 * como el tema o el tamaño de tanda, no algo del servidor.
 *
 * Qué **no** se guarda: hasta dónde estaba expandido el árbol adentro de un servidor. Al abrir la
 * aplicación no hay ninguna conexión abierta, así que restaurar ese camino sería reconectar y
 * disparar una consulta por nivel sin que nadie lo haya pedido.
 */

const COLLAPSED_KEY = "pgforge.folders.collapsed";
const EMPTY_KEY = "pgforge.folders.empty";

/**
 * Con qué nombre se recuerda la sección de los servidores sueltos, que no tiene ninguno. La cadena
 * vacía no puede chocar con una carpeta de verdad: `normalize_group` no deja guardar una sin nombre.
 */
export const LOOSE_GROUP = "";

function stored(key: string): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value.filter((name) => typeof name === "string") : [];
  } catch {
    // Ni un valor corrupto ni la falta de `localStorage` —los tests corren en Node— pueden impedir
    // que esto se cargue: se descarta y se escribe de nuevo en cuanto el usuario toque una carpeta.
    return [];
  }
}

function save(key: string, names: string[]) {
  localStorage.setItem(key, JSON.stringify(names));
}

class Folders {
  /** Carpetas que el usuario dejó cerradas. */
  private collapsed = $state<string[]>(stored(COLLAPSED_KEY));

  /** Carpetas sin ningún servidor adentro. Existen solo acá: son un nombre y nada más. */
  empty = $state<string[]>(stored(EMPTY_KEY));

  /** Si la carpeta tiene que dibujarse abierta. Una carpeta nueva arranca abierta. */
  isOpen(name: string): boolean {
    return !this.collapsed.includes(name);
  }

  setOpen(name: string, open: boolean) {
    const without = this.collapsed.filter((item) => item !== name);
    this.collapsed = open ? without : [...without, name];
    save(COLLAPSED_KEY, this.collapsed);
  }

  /** Recuerda una carpeta vacía, que del lado de Rust no deja rastro. */
  remember(name: string) {
    if (this.empty.includes(name)) return;
    this.empty = [...this.empty, name];
    save(EMPTY_KEY, this.empty);
  }

  /** La deja de recordar: se llenó, se renombró o se deshizo. */
  forget(name: string) {
    if (!this.empty.includes(name)) return;
    this.empty = this.empty.filter((item) => item !== name);
    save(EMPTY_KEY, this.empty);
  }

  /**
   * Deja como vacías exactamente las de la lista. La usa el árbol al rearmarse: una carpeta vacía
   * que acaba de recibir un servidor ya existe entre los perfiles y sale de acá.
   */
  keepEmpty(names: string[]) {
    if (names.length === this.empty.length && names.every((name) => this.empty.includes(name))) {
      return;
    }
    this.empty = [...names];
    save(EMPTY_KEY, this.empty);
  }

  /** Sigue al renombrado de una carpeta, para no perder que estaba cerrada. */
  rename(from: string, to?: string) {
    if (this.collapsed.includes(from)) {
      this.setOpen(from, true);
      if (to) this.setOpen(to, false);
    }
    if (this.empty.includes(from)) {
      this.forget(from);
      if (to) this.remember(to);
    }
  }
}

export const folders = new Folders();
