import type { TypeChange, TypeField } from "./ipc";

/** Una fila del editor de valores de una enumeración. */
export interface LabelRow {
  /** El valor tal como está en el servidor. Vacío en una fila recién agregada. */
  original: string;
  value: string;
}

/** Una fila del editor de campos de un tipo compuesto. */
export interface FieldRow {
  /** El nombre tal como está en el servidor. Vacío en una fila recién agregada. */
  original: string;
  name: string;
  dataType: string;
}

/**
 * Los cambios de una enumeración que ya existe.
 *
 * PostgreSQL no tiene `DROP VALUE`: sacar un valor exigiría recrear el tipo y todas las columnas
 * que lo usan, así que una fila borrada en pantalla no genera nada y el diálogo lo advierte. Lo que
 * sí se puede es agregar —en una posición— y renombrar.
 *
 * El valor nuevo se ancla `AFTER` el anterior de la lista para que el orden de la pantalla sea el
 * que queda en el tipo; sin ancla, PostgreSQL lo pone al final.
 */
export function enumChanges(
  schema: string,
  name: string,
  before: string[],
  rows: LabelRow[],
): TypeChange[] {
  const changes: TypeChange[] = [];
  let previous: string | null = null;

  for (const row of rows) {
    const value = row.value.trim();
    if (value === "") continue;

    if (row.original === "") {
      changes.push({
        kind: "addEnumValue",
        schema,
        name,
        value,
        position: previous === null ? null : { kind: "after", value: previous },
        ifNotExists: true,
      });
    } else if (row.original !== value && before.includes(row.original)) {
      changes.push({ kind: "renameEnumValue", schema, name, from: row.original, to: value });
    }

    previous = value;
  }

  return changes;
}

/** `true` si alguna fila del servidor desapareció de la pantalla: no se puede, y hay que avisarlo. */
export function droppedLabels(before: string[], rows: LabelRow[]): string[] {
  const kept = new Set(rows.filter((row) => row.original !== "").map((row) => row.original));
  return before.filter((label) => !kept.has(label));
}

/**
 * Los cambios de un tipo compuesto que ya existe.
 *
 * Renombrar un campo no está: `ALTER TYPE … RENAME ATTRIBUTE` existe, pero mezclarlo con el cambio
 * de tipo en la misma fila haría imposible distinguir «renombré» de «borré y agregué». El diálogo
 * deja el nombre fijo en las filas que ya estaban.
 */
export function compositeChanges(
  schema: string,
  name: string,
  before: TypeField[],
  rows: FieldRow[],
): TypeChange[] {
  const changes: TypeChange[] = [];
  const kept = new Set<string>();

  for (const row of rows) {
    const fieldName = row.name.trim();
    const dataType = row.dataType.trim();
    if (fieldName === "" || dataType === "") continue;

    if (row.original === "") {
      changes.push({
        kind: "addCompositeField",
        schema,
        name,
        field: { name: fieldName, dataType, collation: null },
      });
      continue;
    }

    kept.add(row.original);
    const previous = before.find((field) => field.name === row.original);
    if (previous && previous.dataType !== dataType) {
      changes.push({
        kind: "alterCompositeFieldType",
        schema,
        name,
        field: row.original,
        dataType,
        collation: null,
        cascade: false,
      });
    }
  }

  for (const field of before) {
    if (!kept.has(field.name)) {
      changes.push({
        kind: "dropCompositeField",
        schema,
        name,
        field: field.name,
        cascade: false,
      });
    }
  }

  return changes;
}
