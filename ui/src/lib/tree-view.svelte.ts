import { hiddenKey, parseHidden, type HiddenMap } from "./hidden-schemas";
import type { OrderMode } from "./tree-order";

const ORDER_KEY = "pgforge.tree.order";
const GROUP_KEY = "pgforge.tree.group";
const HIDDEN_KEY = "pgforge.tree.hiddenschemas";

function readOrder(): OrderMode {
  try {
    const saved = localStorage.getItem(ORDER_KEY);
    return saved === "size" || saved === "rows" ? saved : "name";
  } catch {
    return "name";
  }
}

function readGroup(): boolean {
  try {
    return localStorage.getItem(GROUP_KEY) === "1";
  } catch {
    return false;
  }
}

function readHidden(): HiddenMap {
  try {
    return parseHidden(localStorage.getItem(HIDDEN_KEY));
  } catch {
    return {};
  }
}

function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Sin `localStorage` rige solo esta sesión.
  }
}

/**
 * Cómo se muestran las relaciones de una carpeta: el orden y si se agrupan por prefijo. Es una
 * preferencia de la aplicación —la misma para todas las carpetas de tablas y vistas—, como el
 * tamaño de letra: quien ordena por tamaño en una base lo quiere en la siguiente.
 */
class TreeView {
  order = $state<OrderMode>(readOrder());
  group = $state(readGroup());

  setOrder(mode: OrderMode) {
    this.order = mode;
    store(ORDER_KEY, mode);
  }

  setGroup(on: boolean) {
    this.group = on;
    store(GROUP_KEY, on ? "1" : "0");
  }
}

export const treeView = new TreeView();

/** Los esquemas ocultos de cada base. */
class HiddenSchemas {
  map = $state.raw<HiddenMap>(readHidden());

  get(profileId: string, database: string): string[] {
    return this.map[hiddenKey(profileId, database)] ?? [];
  }

  set(profileId: string, database: string, names: string[]) {
    const key = hiddenKey(profileId, database);
    const next = { ...this.map };
    if (names.length === 0) delete next[key];
    else next[key] = names;
    this.map = next;
    store(HIDDEN_KEY, JSON.stringify(next));
  }
}

export const hiddenSchemas = new HiddenSchemas();
