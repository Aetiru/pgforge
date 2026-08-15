/**
 * Comparación de esquemas entre dos servidores.
 *
 * El informe y el SQL de sincronización llegan juntos en un solo `invoke`: salen de la misma
 * lectura, y pedirlos por separado haría el doble de consultas para responder lo mismo —además de
 * arriesgarse a que el script no se corresponda con el informe que se está mirando—.
 *
 * Nada de esto se ejecuta desde acá. No hay un `compareApply`: el script se copia o se abre en una
 * pestaña de consulta, y quien lo corre es el usuario.
 */

import { invoke } from "./core";

/** Qué clase de objeto difiere. Espejo de `compare::ObjectKind`. */
export type CompareObject =
  | "table"
  | "partitionedTable"
  | "foreignTable"
  | "view"
  | "materializedView"
  | "sequence"
  | "enum"
  | "composite"
  | "domain"
  | "range";

/** De qué lado está lo que se encontró. */
export type DiffStatus = "onlySource" | "onlyTarget" | "different";

/** Qué parte de un objeto difiere. */
export type DiffDetailKind = "column" | "constraint" | "index" | "member" | "property";

export interface DiffDetail {
  kind: DiffDetailKind;
  name: string;
  status: DiffStatus;
  /** Ausente cuando de ese lado no existe. */
  source?: string;
  target?: string;
}

export interface DiffEntry {
  kind: CompareObject;
  name: string;
  status: DiffStatus;
  /** El objeto entero de cada lado, para mostrarlos enfrentados. */
  sourceDdl?: string;
  targetDdl?: string;
  details: DiffDetail[];
}

/** De dónde salió cada lado de la comparación. */
export interface CompareSideInfo {
  /** Nombre del perfil de conexión. */
  server: string;
  database: string;
  schema: string;
  version: string;
}

export interface SchemaDiff {
  source: CompareSideInfo;
  target: CompareSideInfo;
  entries: DiffEntry[];
  /** Cuántos objetos resultaron idénticos. No se listan; sin el número no se sabe si se leyó algo. */
  equal: number;
}

/** Qué hace la sentencia con lo que ya existe. */
export type SyncRisk = "safe" | "review" | "destructive";
export type SyncAction = "create" | "alter" | "drop";

export interface SyncStatement {
  object: CompareObject;
  /** Objeto al que pertenece, el mismo nombre con el que aparece en el informe. */
  name: string;
  action: SyncAction;
  risk: SyncRisk;
  sql: string;
  /** Lo que hay que saber antes de correrla. */
  note?: string;
}

export interface SyncPlan {
  statements: SyncStatement[];
  /** Diferencias que no se arreglan con un ALTER, con el motivo. */
  warnings: string[];
}

export interface Comparison {
  diff: SchemaDiff;
  plan: SyncPlan;
}

/** Un lado de la comparación: qué esquema, de qué base, de qué servidor conectado. */
export interface CompareSide {
  id: string;
  database: string;
  schema: string;
}

/**
 * Compara dos esquemas. El origen es el estado que se quiere; el destino, el que habría que llevar
 * hasta ahí. Los dos servidores tienen que estar conectados.
 */
export const schemaCompare = (source: CompareSide, target: CompareSide) =>
  invoke<Comparison>("schema_compare", { source, target });

/** Los esquemas de una base, para elegir contra cuál comparar sin desplegar el árbol. */
export const schemaNames = (id: string, database: string) =>
  invoke<string[]>("schema_names", { id, database });
