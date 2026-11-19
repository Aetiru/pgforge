/**
 * Qué ayudas del editor están encendidas. Preferencia de la aplicación (no de la pestaña ni del
 * servidor): quien apaga la revisión una vez la quiere apagada en todas.
 *
 * Las tres arrancan encendidas. Se guardan en `localStorage`, y si no está disponible (ventana
 * privada, datos bloqueados) el editor funciona igual con los valores de fábrica.
 */

const KEYS = {
  check: "pgforge.editor.check",
  lint: "pgforge.editor.lint",
  ghost: "pgforge.editor.ghost",
} as const;

type Pref = keyof typeof KEYS;

function read(key: string): boolean {
  try {
    return localStorage.getItem(key) !== "off";
  } catch {
    return true;
  }
}

function write(key: string, on: boolean) {
  try {
    localStorage.setItem(key, on ? "on" : "off");
  } catch {
    // Sin almacenamiento el valor vale solo hasta cerrar la ventana.
  }
}

class EditorPrefs {
  /** Revisar la sentencia con el servidor mientras se escribe (sin ejecutarla). */
  check = $state(read(KEYS.check));
  /** Avisos locales de consultas riesgosas. */
  lint = $state(read(KEYS.lint));
  /** Texto fantasma con lo que ya se ejecutó antes. */
  ghost = $state(read(KEYS.ghost));

  set(pref: Pref, on: boolean) {
    this[pref] = on;
    write(KEYS[pref], on);
  }
}

export const editorPrefs = new EditorPrefs();
