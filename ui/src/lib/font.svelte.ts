/**
 * Fuente monoespaciada: la del código y los datos.
 *
 * `app.css` define `--font-mono` con Source Code Pro como valor de nacimiento; acá se la pisa con
 * una variable en línea sobre `documentElement`, igual que el tamaño de letra del SQL
 * (`editor.svelte.ts`), para que Tailwind, CodeMirror y todo lo demás sigan leyendo la misma
 * variable sin que cada componente sepa que hay una preferencia detrás.
 *
 * Dos opciones y no una lista abierta: son las dos fuentes monoespaciadas que vienen empaquetadas
 * (`main.ts`) y elegir una que no está instalada ni empaquetada dejaría la interfaz en la fuente de
 * respaldo del sistema sin ningún aviso.
 */

export type FontChoice = "source-code-pro" | "jetbrains-mono";

const KEY = "pgforge.font";

const STACKS: Record<FontChoice, string> = {
  "source-code-pro": '"Source Code Pro", "Cascadia Code", ui-monospace, monospace',
  "jetbrains-mono": '"JetBrains Mono", "Cascadia Code", ui-monospace, monospace',
};

export const FONT_LABELS: Record<FontChoice, string> = {
  "source-code-pro": "Source Code Pro",
  "jetbrains-mono": "JetBrains Mono",
};

/** Sin `wstorage`: es preferencia de la aplicación entera, no de una ventana particular. */
function stored(): FontChoice {
  try {
    const value = localStorage.getItem(KEY);
    return value === "jetbrains-mono" ? "jetbrains-mono" : "source-code-pro";
  } catch {
    return "source-code-pro";
  }
}

class Font {
  choice = $state<FontChoice>(stored());

  constructor() {
    this.apply();
  }

  private apply() {
    const stack = STACKS[this.choice];
    // Solo la monoespaciada: `--font-sans` es la de la interfaz (rótulos, botones, menús) y queda
    // fija; lo que se elige acá es la letra del código y de los datos.
    document.documentElement.style.setProperty("--font-mono", stack);
  }

  set(choice: FontChoice) {
    this.choice = choice;
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      // Nada que hacer sin `localStorage`: la preferencia no se recuerda esta vez.
    }
    this.apply();
  }
}

export const font = new Font();
