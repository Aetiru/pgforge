/**
 * Lógica pura del formulario de columnas: qué `TableChange` sale de lo que hay escrito en pantalla.
 *
 * Vive fuera de `ColumnDialog.svelte` por lo mismo que `role-form.ts`: es la parte verificable sin
 * ventana y donde un error se propaga callado. Editar una columna no manda «la columna como quedó»
 * sino un cambio por cada cosa que se tocó, y el orden importa —el renombre va primero, porque los
 * que siguen se ejecutan después, en la misma transacción, y tienen que hablar del nombre nuevo—.
 */

import type { Identity, TableChange, TableColumn } from "./ipc";

/** Tipos comunes, solo como sugerencia: el que valida un tipo de verdad es el servidor. */
export const COMMON_TYPES = [
  "bigint",
  "integer",
  "smallint",
  "text",
  "varchar(255)",
  "numeric(12,2)",
  "boolean",
  "timestamptz",
  "date",
  "uuid",
  "jsonb",
  "bytea",
];

export const IDENTITY_OPTIONS: { value: Identity | ""; label: string }[] = [
  { value: "", label: "Ninguna" },
  { value: "always", label: "Siempre" },
  { value: "byDefault", label: "Por defecto" },
];

export interface ColumnForm {
  name: string;
  typeName: string;
  notNull: boolean;
  /** Expresión SQL tal como se escribe; vacío es «sin default». */
  default: string;
  /** Solo al dar de alta: una columna existente no cambia de identidad desde acá. */
  identity: Identity | "";
  /** Solo al cambiar de tipo, cuando la conversión no es implícita. */
  using: string;
}

/** La copia editable inicial. El diálogo la toma una sola vez, con `untrack`. */
export function columnForm(column: TableColumn | null): ColumnForm {
  return {
    name: column?.name ?? "",
    typeName: column?.typeName ?? "",
    notNull: column?.notNull ?? false,
    default: column?.default ?? "",
    identity: "",
    using: "",
  };
}

export function validateColumn(form: ColumnForm): string | null {
  if (!form.name.trim()) return "Poné un nombre para la columna.";
  if (!form.typeName.trim()) return "Poné un tipo para la columna.";
  return null;
}

/** Los cambios pendientes. En alta siempre hay uno; en edición, solo lo que se tocó. */
export function columnChanges(
  form: ColumnForm,
  target: { schema: string; table: string },
  column: TableColumn | null,
): TableChange[] {
  const { schema, table } = target;
  const name = form.name.trim();
  const typeName = form.typeName.trim();

  if (!column) {
    return [
      {
        kind: "addColumn",
        schema,
        table,
        column: {
          name,
          typeName,
          notNull: form.notNull,
          // Una columna de identidad la llena el servidor: un default suyo no tendría dónde aplicarse.
          default: form.identity ? null : form.default.trim() || null,
          identity: form.identity || null,
        },
      },
    ];
  }

  const out: TableChange[] = [];
  // El renombre va primero: los pasos siguientes ya tienen que referirse al nombre nuevo, porque se
  // ejecutan en orden dentro de la misma transacción.
  let current = column.name;
  if (name !== column.name) {
    out.push({ kind: "renameColumn", schema, table, column: current, newName: name });
    current = name;
  }
  if (typeName !== column.typeName) {
    out.push({
      kind: "alterColumnType",
      schema,
      table,
      column: current,
      typeName,
      using: form.using.trim() || null,
    });
  }
  if (form.notNull !== column.notNull) {
    out.push({ kind: "setColumnNotNull", schema, table, column: current, notNull: form.notNull });
  }
  const original = column.default ?? "";
  if (form.default.trim() !== original.trim()) {
    out.push({
      kind: "setColumnDefault",
      schema,
      table,
      column: current,
      default: form.default.trim() || null,
    });
  }
  return out;
}
