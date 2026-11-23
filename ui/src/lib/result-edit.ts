/**
 * Editar el resultado de una consulta que sale de una sola tabla.
 *
 * El servidor dice de qué tabla sale cada columna (`EditSource`); acá se decide qué celdas se pueden
 * tocar y se arman los `Change` que ya sabe aplicar `data::edit`: cada `UPDATE` lleva la clave de la
 * fila y los valores originales de lo que cambia, así que si otro tocó la fila en el medio se
 * informa un conflicto en vez de pisarla. Es puro y con Vitest.
 */
import { t } from "./i18n.svelte";
import type { Change, EditSource, TableShape } from "./ipc";

export interface EditPlan {
  shape: TableShape;
  /** Nombre en la tabla de cada columna del resultado, o `null`. */
  mapped: (string | null)[];
  /** Posición, en el resultado, de cada columna de la clave de la tabla. */
  keyAt: number[];
  /** Qué columnas del resultado se pueden editar. */
  editable: boolean[];
}

/** Cambios por fila y por columna del resultado. `null` es un NULL de la base. */
export type Edits = ReadonlyMap<number, ReadonlyMap<number, string | null>>;

/**
 * El plan de edición, o el motivo por el que no se ofrece. La clave tiene que estar **entera** en el
 * resultado: sin ella no hay forma de señalar la fila, que es lo único que hace seguro el `UPDATE`.
 */
export function planEdit(shape: TableShape, source: EditSource): EditPlan | string {
  if (shape.readOnly) return shape.readOnly;
  if (!shape.key) return t("La tabla no tiene clave con la cual señalar una fila.");

  const keyAt: number[] = [];
  for (const name of shape.key.columns) {
    const at = source.columns.indexOf(name);
    if (at === -1) {
      return t("Falta la columna «{name}» de la clave en el resultado: sin ella no se sabe qué fila es cada una. Agregala al SELECT para poder editar.", { name });
    }
    keyAt.push(at);
  }

  const editable = source.columns.map((name) => {
    if (name === null) return false;
    const column = shape.columns.find((candidate) => candidate.name === name);
    // Las generadas y las de identidad las calcula el servidor.
    return column !== undefined && !column.generated;
  });

  return { shape, mapped: source.columns, keyAt, editable };
}

/** Cuántas filas tienen al menos un cambio. */
export function editedRows(edits: Edits): number {
  let count = 0;
  for (const cells of edits.values()) if (cells.size > 0) count++;
  return count;
}

/** Los `UPDATE` que salen de las ediciones. Vuelve a su valor original = no es un cambio. */
export function buildChanges(
  plan: EditPlan,
  rows: readonly (readonly (string | null)[])[],
  edits: Edits,
): Change[] {
  const changes: Change[] = [];

  for (const [rowIndex, cells] of edits) {
    const row = rows[rowIndex];
    if (!row) continue;

    const original: Record<string, string | null> = {};
    const updated: Record<string, string | null> = {};
    for (const [column, value] of cells) {
      const name = plan.mapped[column];
      if (name === null || name === undefined || !plan.editable[column]) continue;
      if (value === row[column]) continue;
      original[name] = row[column];
      updated[name] = value;
    }
    if (Object.keys(updated).length === 0) continue;

    changes.push({
      kind: "update",
      key: plan.keyAt.map((at) => row[at] ?? ""),
      original,
      changes: updated,
    });
  }

  return changes;
}

/** Registra un cambio de celda. Devolver el valor original lo quita. */
export function withEdit(
  edits: Edits,
  rows: readonly (readonly (string | null)[])[],
  rowIndex: number,
  column: number,
  value: string | null,
): Map<number, Map<number, string | null>> {
  const next = new Map([...edits].map(([row, cells]) => [row, new Map(cells)] as const));
  const cells = next.get(rowIndex) ?? new Map<number, string | null>();

  if (value === rows[rowIndex]?.[column]) cells.delete(column);
  else cells.set(column, value);

  if (cells.size === 0) next.delete(rowIndex);
  else next.set(rowIndex, cells);
  return next;
}
