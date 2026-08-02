/**
 * Tema claro u oscuro.
 *
 * Por omisión sigue al sistema, que es lo que espera cualquiera que ya eligió su preferencia en
 * Windows. Pero un cliente de base de datos se usa horas seguidas y muchas veces junto a otras
 * herramientas: poder forzar uno de los dos es parte de que la ventana sea cómoda.
 *
 * La preferencia se guarda; lo que se aplica al documento es siempre el tema ya resuelto, para que
 * Tailwind, CodeMirror y los gráficos lean un único atributo en vez de repetir la decisión.
 */

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const KEY = "pgforge.theme";

const media = window.matchMedia("(prefers-color-scheme: dark)");

function stored(): ThemePreference {
  const value = localStorage.getItem(KEY);
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

function resolve(preference: ThemePreference): ResolvedTheme {
  if (preference === "system") return media.matches ? "dark" : "light";
  return preference;
}

class Theme {
  preference = $state<ThemePreference>(stored());
  resolved = $state<ResolvedTheme>(resolve(stored()));

  constructor() {
    this.apply();
    // Si la preferencia es «automático», seguir al sistema mientras la ventana está abierta.
    media.addEventListener("change", () => {
      if (this.preference === "system") this.apply();
    });
  }

  private apply() {
    this.resolved = resolve(this.preference);
    document.documentElement.dataset.theme = this.resolved;
  }

  set(preference: ThemePreference) {
    this.preference = preference;
    localStorage.setItem(KEY, preference);
    this.apply();
  }

  /** Recorre los tres estados en orden; es lo que hace el botón de la barra. */
  cycle() {
    const order: ThemePreference[] = ["system", "light", "dark"];
    this.set(order[(order.indexOf(this.preference) + 1) % order.length]);
  }
}

export const theme = new Theme();
