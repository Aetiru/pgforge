import { pickLocale, translate, type Dictionary, type Locale, type Params } from "./i18n";
import { en } from "./locales/en";

const KEY = "pgforge.locale";

const DICTIONARIES: Record<Locale, Dictionary | null> = { es: null, en };

/** Sin `wstorage`: el idioma es de la aplicación entera, no de una ventana. */
function stored(): Locale {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(KEY);
  } catch {
    // Sin almacenamiento (ventana privada, datos bloqueados): se decide por el sistema.
  }
  return pickLocale(saved, typeof navigator === "undefined" ? undefined : navigator.language);
}

class I18n {
  current = $state<Locale>(stored());

  constructor() {
    this.apply();
  }

  set(locale: Locale) {
    this.current = locale;
    try {
      localStorage.setItem(KEY, locale);
    } catch {
      // Es una preferencia: perderla no impide usar la aplicación.
    }
    this.apply();
  }

  /** Lectores de pantalla y el corrector del webview dependen de `lang`. */
  private apply() {
    if (typeof document !== "undefined") document.documentElement.lang = this.current;
  }
}

export const i18n = new I18n();

/**
 * Traduce un texto escrito en español. Lee `i18n.current`, así que dentro de una plantilla o de un
 * `$derived` se vuelve a evaluar al cambiar de idioma. **No** llamarla al cargar el módulo para
 * armar una constante: quedaría fija en el idioma de ese momento (hay que usar una función).
 */
export function t(source: string, params?: Params): string {
  return translate(DICTIONARIES[i18n.current], source, params);
}
