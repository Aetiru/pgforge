import { DEFAULT_FORMAT_OPTIONS, sqlFormat, type FormatOptions } from "./ipc";
import { parseStoredFormatOptions, sanitizeFormatOptions } from "./format-options";

/**
 * El estilo del formateador, una preferencia de la aplicación: quien lo ajusta una vez lo quiere
 * igual en todas las pestañas, y es lo que hace que el SQL de un equipo salga siempre con la misma
 * forma. Mismo criterio que `paging` y `autoFormat`: `localStorage` sin `wstorage`.
 */
const KEY = "pgforge.format";

function read(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

class FormatPrefs {
  options = $state<FormatOptions>(parseStoredFormatOptions(read()));

  set(next: Partial<FormatOptions>) {
    this.options = sanitizeFormatOptions({ ...this.options, ...next });
    try {
      localStorage.setItem(KEY, JSON.stringify(this.options));
    } catch {
      // Sin almacenamiento el estilo vale por esta sesión, nada más.
    }
  }

  reset() {
    this.set({ ...DEFAULT_FORMAT_OPTIONS });
  }
}

export const formatPrefs = new FormatPrefs();

/**
 * Formatea con el estilo guardado. Es el único camino que usan el atajo, el botón, el comando de la
 * paleta y el autoformateo: sin pasar todos por acá, uno de ellos seguiría ordenando con el estilo
 * de fábrica.
 */
export function formatSql(sql: string): Promise<string> {
  return sqlFormat(sql, $state.snapshot(formatPrefs.options));
}
