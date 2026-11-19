import { parseViews, viewsKey, withView, withoutView, type GridView, type ViewMap } from "./grid-views";

const KEY = "pgforge.gridviews";

function read(): ViewMap {
  try {
    return parseViews(localStorage.getItem(KEY));
  } catch {
    return {};
  }
}

/** Vistas guardadas de la grilla, por tabla. Preferencia de esta computadora. */
class GridViews {
  map = $state.raw<ViewMap>(read());

  of(profileId: string, database: string, schema: string, table: string): GridView[] {
    return this.map[viewsKey(profileId, database, schema, table)] ?? [];
  }

  save(profileId: string, database: string, schema: string, table: string, view: GridView) {
    const key = viewsKey(profileId, database, schema, table);
    this.write({ ...this.map, [key]: withView(this.map[key] ?? [], view) });
  }

  remove(profileId: string, database: string, schema: string, table: string, name: string) {
    const key = viewsKey(profileId, database, schema, table);
    const next = { ...this.map, [key]: withoutView(this.map[key] ?? [], name) };
    if (next[key].length === 0) delete next[key];
    this.write(next);
  }

  private write(next: ViewMap) {
    this.map = next;
    try {
      localStorage.setItem(KEY, JSON.stringify(next));
    } catch {
      // Sin `localStorage` valen hasta cerrar la ventana.
    }
  }
}

export const gridViews = new GridViews();
