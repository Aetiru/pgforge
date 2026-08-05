import { confirmMutation, isReadOnly } from "./access.svelte";
import { Tab, tabs } from "./tabs.svelte";
import {
  dataApply,
  dataOpen,
  dataPage,
  dataPreview,
  describeError,
  type Change,
  type Cursor,
  type PreviewStatement,
  type RowValues,
  type TableShape,
} from "./ipc";

/**
 * Una fila cargada, con su identidad y lo que se editó encima.
 *
 * Los valores originales se conservan aparte de los editados por dos motivos: para poder descartar
 * un cambio sin volver a consultar, y porque el `UPDATE` los manda al servidor para detectar que
 * otro tocó la fila en el medio.
 */
export interface Row {
  /** Posición dentro de lo cargado. Es la identidad para dibujar, no para el servidor. */
  index: number;
  original: (string | null)[];
  /** Solo las celdas tocadas, por índice de columna. */
  edited: Map<number, string | null>;
  /** Valores de clave con los que se leyó, para poder señalarla aunque se edite la clave. */
  key: string[];
  /** Alta local todavía no guardada. */
  isNew: boolean;
  deleted: boolean;
}

export class DataTab extends Tab {
  readonly kind = "data" as const;

  readonly oid: number;

  shape = $state.raw<TableShape | null>(null);
  /** Las filas se reemplazan enteras al agregar una página, nunca se mutan por dentro. */
  rows = $state.raw<Row[]>([]);
  cursor = $state.raw<Cursor | null>(null);

  loading = $state(false);
  saving = $state(false);
  /** `true` cuando ya se trajo la última página. */
  complete = $state(false);
  error = $state<string | null>(null);
  preview = $state.raw<PreviewStatement[] | null>(null);

  constructor(profileId: string, database: string, title: string, oid: number) {
    super(profileId, database, title);
    this.oid = oid;
  }

  get editable(): boolean {
    // Dos motivos distintos para lo mismo: la relación no tiene con qué identificar una fila, o el
    // perfil entero se abrió en solo lectura. La grilla los muestra igual.
    return this.shape !== null && this.shape.readOnly === null && !isReadOnly(this.profileId);
  }

  get pending(): number {
    return this.rows.filter((row) => row.deleted || row.isNew || row.edited.size > 0).length;
  }

  /** El valor que hay que mostrar en una celda: lo editado si se tocó, si no lo original. */
  value(row: Row, column: number): string | null {
    return row.edited.has(column) ? (row.edited.get(column) ?? null) : row.original[column];
  }

  async load() {
    this.loading = true;
    this.error = null;

    try {
      this.shape = await dataOpen(this.profileId, this.oid, this.database);
      this.rows = [];
      this.cursor = null;
      this.complete = false;
      await this.more();
    } catch (error) {
      this.error = describeError(error);
    } finally {
      this.loading = false;
    }
  }

  /** Trae la página siguiente. Es lo que dispara el scroll al acercarse al final. */
  async more() {
    const shape = this.shape;
    if (!shape || this.complete || this.loading) return;

    this.loading = true;
    try {
      const page = await dataPage(this.profileId, shape, this.cursor, undefined, this.database);
      const base = this.rows.length;

      const nuevas = page.rows.map((cells, offset) => ({
        index: base + offset,
        original: cells,
        edited: new Map<number, string | null>(),
        key: keyOf(shape, page.columns, cells),
        isNew: false,
        deleted: false,
      }));

      this.rows = [...this.rows, ...nuevas];
      this.cursor = page.next;
      this.complete = page.next === null;
    } catch (error) {
      this.error = describeError(error);
      // Sin esto el scroll seguiría pidiendo la misma página fallida en cada cuadro.
      this.complete = true;
    } finally {
      this.loading = false;
    }
  }

  edit(row: Row, column: number, value: string | null) {
    const original = row.original[column];
    // Volver al valor original no es un cambio: dejarlo marcado haría que «guardar» mande un UPDATE
    // que no cambia nada y que puede fallar por la guarda de concurrencia.
    if (value === original && !row.isNew) {
      row.edited.delete(column);
    } else {
      row.edited.set(column, value);
    }
    // `edited` es un Map dentro de un arreglo `raw`: hay que avisar el cambio reemplazando.
    this.rows = [...this.rows];
  }

  addRow() {
    const shape = this.shape;
    if (!shape) return;

    this.rows = [
      ...this.rows,
      {
        index: this.rows.length,
        original: shape.columns.map(() => null),
        edited: new Map(),
        key: [],
        isNew: true,
        deleted: false,
      },
    ];
  }

  toggleDelete(row: Row) {
    if (row.isNew) {
      // Un alta que nunca se guardó se descarta y listo.
      this.rows = this.rows.filter((item) => item !== row);
      return;
    }
    row.deleted = !row.deleted;
    this.rows = [...this.rows];
  }

  discard() {
    this.rows = this.rows
      .filter((row) => !row.isNew)
      .map((row) => ({ ...row, edited: new Map(), deleted: false }));
    this.preview = null;
  }

  /** Los cambios pendientes, en el formato que entiende el núcleo. */
  changes(): Change[] {
    const shape = this.shape;
    if (!shape) return [];

    const named = (cells: Map<number, string | null>): RowValues =>
      Object.fromEntries(
        [...cells].map(([column, value]) => [shape.columns[column].name, value]),
      );

    const out: Change[] = [];

    for (const row of this.rows) {
      if (row.deleted && !row.isNew) {
        out.push({ kind: "delete", key: row.key });
      } else if (row.isNew) {
        out.push({ kind: "insert", values: named(row.edited) });
      } else if (row.edited.size > 0) {
        // Los originales van solo de las columnas que se tocan: comparar la fila entera haría
        // fallar el guardado por un cambio ajeno en una columna que a este usuario no le importa.
        const original: RowValues = {};
        for (const column of row.edited.keys()) {
          original[shape.columns[column].name] = row.original[column];
        }
        out.push({ kind: "update", key: row.key, original, changes: named(row.edited) });
      }
    }

    return out;
  }

  async showPreview() {
    const shape = this.shape;
    if (!shape) return;

    this.error = null;
    try {
      this.preview = await dataPreview(shape, this.changes());
    } catch (error) {
      this.error = describeError(error);
    }
  }

  async save() {
    const shape = this.shape;
    const changes = this.changes();
    if (!shape || changes.length === 0) return;
    if (!(await confirmMutation(this.profileId, `Se van a aplicar ${changes.length} cambios.`))) {
      return;
    }

    this.saving = true;
    this.error = null;

    try {
      await dataApply(this.profileId, shape, changes, this.database);
      this.preview = null;
      // Se relee todo: los valores por omisión, las columnas generadas y los disparadores hacen
      // que lo que quedó en la base no sea necesariamente lo que se mandó.
      await this.load();
    } catch (error) {
      this.error = describeError(error);
    } finally {
      this.saving = false;
    }
  }
}

/** Los valores de clave de una fila recién leída. */
function keyOf(shape: TableShape, columns: string[], cells: (string | null)[]): string[] {
  if (!shape.key) return [];

  return shape.key.columns.map((name) => {
    const index = columns.indexOf(name);
    return cells[index] ?? "";
  });
}

/** Abre la grilla de una tabla y trae la primera página. */
export async function openData(
  profileId: string,
  database: string,
  title: string,
  oid: number,
): Promise<DataTab> {
  const tab = tabs.add(new DataTab(profileId, database, title, oid));
  await tab.load();
  return tab;
}
