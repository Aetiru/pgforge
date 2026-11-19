import { invoke } from "./core";

// ---------------------------------------------------------------------------
// Editor de SQL: revisar sin ejecutar, ayudas al escribir y versiones del texto
// ---------------------------------------------------------------------------

/**
 * El veredicto de revisar una sentencia con el servidor sin ejecutarla. `skipped` no es un fallo y
 * no se muestra: una transacción abierta, varias sentencias, una pestaña ocupada.
 */
export type CheckResult =
  | { status: "ok" }
  | {
      status: "error";
      message: string;
      /** Dónde empieza el problema dentro del texto enviado: en caracteres, con base 0. */
      position: number | null;
      hint: string | null;
    }
  | { status: "skipped"; reason: string };

export const queryCheck = (tabId: string, sql: string) =>
  invoke<CheckResult>("query_check", { tabId, sql });

export interface ColumnValues {
  /** `enum`: etiquetas del tipo; `frequent`: las más comunes según las estadísticas; `none`: nada. */
  kind: "enum" | "frequent" | "none";
  values: { value: string; frequency: number | null }[];
}

/** Valores probables de una columna. Sin `ANALYZE` (o sin permiso) vuelve `kind: "none"`, no un error. */
export const columnValues = (
  id: string,
  database: string,
  schema: string,
  table: string,
  column: string,
) => invoke<ColumnValues>("column_values", { id, database, schema, table, column });

export interface RoutineSignature {
  schema: string;
  name: string;
  /** Como los escribiría `CREATE FUNCTION`: `field text, source timestamp with time zone`. */
  arguments: string;
  /** `null` en un procedimiento. */
  result: string | null;
  comment: string | null;
  kind: "function" | "procedure" | "aggregate" | "window";
}

/** Las sobrecargas de una rutina por nombre; con `schema` en `null` se buscan en todos los esquemas. */
export const routineSignatures = (
  id: string,
  database: string,
  schema: string | null,
  name: string,
) => invoke<RoutineSignature[]>("routine_signatures", { id, database, schema, name });

/** Consultas ya ejecutadas con éxito que empiezan exactamente con `prefix`, la más nueva primero. */
export const historySuggest = (prefix: string, limit?: number) =>
  invoke<string[]>("history_suggest", { prefix, limit: limit ?? null });

/** Estilo del formateador de SQL. Los identificadores nunca cambian de caso: solo las palabras clave. */
export interface FormatOptions {
  keywordCase: "upper" | "lower" | "preserve";
  /** Con qué se sangra cada nivel: dos espacios, cuatro o un tabulador. */
  indent: "2" | "4" | "tab";
  /** Líneas en blanco entre sentencias (0 a 3; el núcleo recorta lo que pase de 3). */
  linesBetweenQueries: number;
  /** La coma al principio de cada línea (`, columna`) en vez de al final. */
  commaFirst: boolean;
}

/** Lo que el formateador hace sin opciones: no cambia la salida de antes. */
export const DEFAULT_FORMAT_OPTIONS: FormatOptions = {
  keywordCase: "upper",
  indent: "4",
  linesBetweenQueries: 1,
  commaFirst: false,
};

export interface VersionMeta {
  id: number;
  /** Segundos desde el epoch. */
  at: number;
  /** Largo del texto, en caracteres. */
  size: number;
  /** El principio del texto en una sola línea. */
  preview: string;
}

/** Guarda una copia del texto de la pestaña `key`; `null` si no hacía falta (igual a la última, vacío o enorme). */
export const versionSave = (key: string, text: string) =>
  invoke<number | null>("version_save", { key, text });

/** Las copias de una pestaña, la más nueva primero. */
export const versionList = (key: string) => invoke<VersionMeta[]>("version_list", { key });

export const versionGet = (id: number) => invoke<string | null>("version_get", { id });

export const versionClear = (key: string) => invoke<void>("version_clear", { key });
