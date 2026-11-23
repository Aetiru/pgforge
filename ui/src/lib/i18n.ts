/**
 * Traducción de la interfaz: lógica pura (sin estado, probada con Vitest).
 *
 * El texto en español **es** la clave: `t("Guardar")`. Español es el idioma de origen, así que no
 * hay diccionario `es` que se desincronice del código, y una traducción que falte se ve en español
 * en vez de como una clave rota. Los demás idiomas son un diccionario `español -> traducción`
 * (`locales/<idioma>/`). Los parámetros van como `{nombre}`.
 */

export type Locale = "es" | "en";

export const LOCALES: { value: Locale; label: string }[] = [
  { value: "es", label: "Español" },
  { value: "en", label: "English" },
];

export type Params = Record<string, string | number>;
export type Dictionary = Record<string, string>;

/** Reemplaza `{nombre}`; un parámetro que no viene queda escrito tal cual, a la vista. */
export function interpolate(text: string, params?: Params): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

/** Sin diccionario (español) o sin entrada, vale el texto de origen. */
export function translate(dict: Dictionary | null, source: string, params?: Params): string {
  return interpolate((dict && dict[source]) ?? source, params);
}

/**
 * Idioma inicial: el guardado si es válido; si no, el del sistema, con español como respaldo para
 * todo lo que no sea inglés (no hay más idiomas y no hay que dejar a nadie con uno que no existe).
 */
export function pickLocale(stored: string | null, system: string | undefined): Locale {
  if (stored === "es" || stored === "en") return stored;
  return system?.toLowerCase().startsWith("en") ? "en" : "es";
}
