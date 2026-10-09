/**
 * Los colores del SQL: qué paleta, qué peso tienen las palabras clave y si los nombres de tablas y
 * columnas llevan color propio.
 *
 * Es una preferencia de la aplicación entera, como el tema y el tamaño de letra: el editor, el DDL de
 * un objeto y toda vista previa leen los mismos `--cm-*`. Lo que se aplica es una hoja inyectada con
 * esas variables (`paletteCss`) y no un tema de CodeMirror por componente, así claro/oscuro sigue
 * resolviéndose con el atributo `data-theme` del documento y cambiar una paleta repinta sin recrear
 * ningún editor.
 */

import { PALETTES, CUSTOM_ID, colorsOf, defaultPrefs, paletteCss, parsePrefs, type SqlColors, type SqlThemePrefs, type KeywordWeight } from "./sql-palettes";

const KEY = "pgforge.sql.theme";
const STYLE_ID = "pgforge-sql-palette";

function read(): SqlThemePrefs {
  try {
    const raw = localStorage.getItem(KEY);
    return parsePrefs(raw ? JSON.parse(raw) : null);
  } catch {
    return defaultPrefs();
  }
}

class SqlTheme {
  prefs = $state<SqlThemePrefs>(read());

  constructor() {
    this.apply();
  }

  private apply() {
    let style = document.getElementById(STYLE_ID) as HTMLStyleElement | null;
    if (!style) {
      style = document.createElement("style");
      style.id = STYLE_ID;
      document.head.append(style);
    }
    style.textContent = paletteCss(this.prefs);
  }

  private commit(next: SqlThemePrefs) {
    this.prefs = next;
    try {
      localStorage.setItem(KEY, JSON.stringify(next));
    } catch {
      // Sin `localStorage` la preferencia no se recuerda esta vez.
    }
    this.apply();
  }

  /** Elegir «Personalizada» parte de los colores de la que se estaba usando, no de cero. */
  setPalette(id: string) {
    if (id === CUSTOM_ID && this.prefs.palette !== CUSTOM_ID) {
      this.commit({
        ...this.prefs,
        palette: id,
        custom: { light: { ...colorsOf(this.prefs, "light") }, dark: { ...colorsOf(this.prefs, "dark") } },
      });
      return;
    }
    this.commit({ ...this.prefs, palette: id });
  }

  setColor(theme: "light" | "dark", token: keyof SqlColors, value: string) {
    this.commit({
      ...this.prefs,
      palette: CUSTOM_ID,
      custom: { ...this.prefs.custom, [theme]: { ...this.prefs.custom[theme], [token]: value } },
    });
  }

  setWeight(weight: KeywordWeight) {
    this.commit({ ...this.prefs, keywordWeight: weight });
  }

  setCommentItalic(on: boolean) {
    this.commit({ ...this.prefs, commentItalic: on });
  }

  setIdentTint(on: boolean) {
    this.commit({ ...this.prefs, identTint: on });
  }

  reset() {
    this.commit(defaultPrefs());
  }
}

export const sqlTheme = new SqlTheme();
export { PALETTES };
