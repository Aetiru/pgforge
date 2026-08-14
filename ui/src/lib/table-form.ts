/**
 * Lógica pura del formulario de tablas: la lista de columnas escritas y el `createTable` que sale
 * de ella.
 *
 * Lo que se prueba acá es la validación —una tabla sin columnas, o una columna sin tipo, la rechaza
 * el servidor con un error de sintaxis que no dice cuál de las ocho filas del formulario está mal—
 * y que la `key` que Svelte usa para identificar cada fila no se filtre al núcleo.
 */

import type { ColumnDef, TableChange } from "./ipc";

/** Una fila del formulario. `key` es solo para que Svelte identifique la fila; no viaja al núcleo. */
export interface DraftColumn extends ColumnDef {
  key: string;
}

export function blankColumn(): DraftColumn {
  return {
    key: crypto.randomUUID(),
    name: "",
    typeName: "",
    notNull: false,
    default: null,
    identity: null,
  };
}

export function validateTable(name: string, columns: DraftColumn[]): string | null {
  if (!name.trim()) return "Poné un nombre para la tabla.";
  if (columns.length === 0) return "Una tabla necesita al menos una columna.";
  for (const column of columns) {
    if (!column.name.trim()) return "Todas las columnas necesitan un nombre.";
    if (!column.typeName.trim()) return `La columna ${column.name} necesita un tipo.`;
  }
  return null;
}

export function tableChange(schema: string, name: string, columns: DraftColumn[]): TableChange {
  return {
    kind: "createTable",
    schema,
    name: name.trim(),
    columns: columns.map(({ key: _key, ...column }) => column),
  };
}
