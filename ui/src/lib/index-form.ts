/**
 * Lógica pura del formulario de índices: el `IndexDef` que sale de lo escrito en pantalla.
 *
 * Lo delicado no es el SQL —lo arma el núcleo— sino qué se manda vacío y qué se manda en `null`: un
 * nombre en blanco significa «que lo nombre Postgres» y un método en blanco significa «btree», y
 * mandar la cadena vacía en vez de `null` produce un `CREATE INDEX ""` que el servidor rechaza.
 */

import type { IndexDef } from "./ipc";

export const INDEX_METHODS = [
  { value: "", label: "btree (por omisión)" },
  { value: "gin", label: "gin" },
  { value: "gist", label: "gist" },
  { value: "hash", label: "hash" },
  { value: "brin", label: "brin" },
  { value: "spgist", label: "spgist" },
];

export interface IndexForm {
  /** Vacío deja que lo nombre el servidor. */
  name: string;
  unique: boolean;
  /** Vacío es btree. */
  method: string;
  columns: string[];
  /** Predicado del índice parcial, crudo: lo valida el servidor al ejecutar. */
  whereClause: string;
  concurrently: boolean;
}

export function indexForm(): IndexForm {
  return { name: "", unique: false, method: "", columns: [], whereClause: "", concurrently: false };
}

export function validateIndex(form: IndexForm): string | null {
  if (form.columns.length === 0) return "Elegí al menos una columna.";
  return null;
}

export function indexDef(form: IndexForm, target: { schema: string; table: string }): IndexDef {
  return {
    schema: target.schema,
    table: target.table,
    name: form.name.trim() || null,
    unique: form.unique,
    method: form.method || null,
    columns: form.columns,
    whereClause: form.whereClause.trim() || null,
    concurrently: form.concurrently,
  };
}

/** Agrega o saca una columna, conservando el orden en que se fueron eligiendo: es el del índice. */
export function toggleColumn(columns: string[], column: string): string[] {
  return columns.includes(column)
    ? columns.filter((name) => name !== column)
    : [...columns, column];
}
