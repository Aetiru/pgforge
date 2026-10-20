/**
 * Los atajos vigentes: los de fábrica (`shortcuts.ts`) más lo que el usuario reasignó.
 *
 * Es una preferencia de la aplicación entera y no de una ventana, así que va en `localStorage`
 * sin pasar por `wstorage`, igual que `font.svelte.ts` o `paging.svelte.ts`. Lo que se guarda son
 * solo las **diferencias** con la fábrica: si un atajo nuevo se agrega en una versión futura, quien
 * nunca tocó nada lo recibe sin que una copia vieja de la tabla entera se lo tape.
 */

import {
  SHORTCUTS,
  effective,
  findConflicts,
  formatCombo,
  matchesEvent,
  normalizeCombo,
  parseOverrides,
  validateCombo,
  type Overrides,
} from "./shortcuts";

const KEY = "pgforge.shortcuts";

function stored(): Overrides {
  try {
    return parseOverrides(localStorage.getItem(KEY));
  } catch {
    return {};
  }
}

export const MAC = typeof navigator !== "undefined" && /mac/i.test(navigator.platform ?? "");

class ShortcutStore {
  overrides = $state<Overrides>(stored());

  /** Combinación vigente de cada atajo. */
  readonly bindings = $derived(effective(this.overrides));
  readonly conflicts = $derived(findConflicts(this.bindings));

  /** La combinación de un atajo (`null` = sin asignar). */
  combo(id: string): string | null {
    return this.bindings.get(id) ?? null;
  }

  /** Cómo se lee: `Ctrl+Mayús+F`. Para los `title` de los botones y las pistas de la paleta. */
  label(id: string): string {
    return formatCombo(this.combo(id), MAC);
  }

  /** `title` con el atajo entre paréntesis, o el texto solo si no tiene. */
  hint(text: string, id: string): string {
    const combo = this.combo(id);
    return combo ? `${text} (${formatCombo(combo, MAC)})` : text;
  }

  matches(event: KeyboardEvent, id: string): boolean {
    return matchesEvent(event, this.combo(id));
  }

  /**
   * Reasigna. Devuelve el motivo por el que no se pudo, o `null`. Un conflicto **no** impide
   * asignar —puede ser deliberado—, solo se marca en la chuleta.
   */
  set(id: string, combo: string | null): string | null {
    const def = SHORTCUTS.find((item) => item.id === id);
    if (!def || def.fixed) return "Ese atajo no se puede cambiar.";
    let next: string | null = null;
    if (combo !== null) {
      next = normalizeCombo(combo);
      const problem = next === null ? "Esa tecla no se puede usar como atajo." : validateCombo(next);
      if (problem) return problem;
    }
    const overrides = { ...this.overrides };
    // Volver a la fábrica no deja una marca guardada.
    if (next === def.default) delete overrides[id];
    else overrides[id] = next;
    this.save(overrides);
    return null;
  }

  reset(id: string) {
    const overrides = { ...this.overrides };
    delete overrides[id];
    this.save(overrides);
  }

  resetAll() {
    this.save({});
  }

  isCustom(id: string): boolean {
    return id in this.overrides;
  }

  private save(overrides: Overrides) {
    this.overrides = overrides;
    try {
      if (Object.keys(overrides).length === 0) localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, JSON.stringify(overrides));
    } catch {
      // Sin `localStorage` la reasignación vale hasta cerrar la ventana.
    }
  }
}

export const shortcutStore = new ShortcutStore();
