/**
 * Objetos que se editan con vista previa: tablas, índices, vistas, secuencias, tipos, dominios,
 * esquemas, bases y particiones. Cada uno con su `*_preview` y su `*_apply`.
 */

import { invoke, type Channel } from "./core";
import type { TaskEvent } from "./tasks";

// ---------------------------------------------------------------------------
// Estructura de tablas
// ---------------------------------------------------------------------------

export type Identity = "always" | "byDefault";

export interface ColumnDef {
  name: string;
  /** Texto crudo: lo valida el servidor al ejecutar, acá no se interpreta. */
  typeName: string;
  notNull: boolean;
  /** Expresión SQL cruda (p. ej. `now()`), no un literal a escapar. */
  default: string | null;
  identity: Identity | null;
}

/** Qué hacer del otro lado de una foreign key cuando la fila referenciada cambia o desaparece. */
export type RefAction = "cascade" | "setNull" | "setDefault" | "restrict" | "noAction";

/** Postgres no tiene un "ALTER CONSTRAINT": cambiarla es borrarla y agregar una nueva. */
export type ConstraintDef =
  | { kind: "primaryKey"; columns: string[] }
  | { kind: "unique"; columns: string[] }
  | {
      kind: "foreignKey";
      columns: string[];
      refSchema: string;
      refTable: string;
      refColumns: string[];
      onDelete: RefAction | null;
      onUpdate: RefAction | null;
    }
  /** Expresión SQL cruda, misma frontera de confianza que el `default` de una columna. */
  | { kind: "check"; expression: string };

export type TableChange =
  | { kind: "createTable"; schema: string; name: string; columns: ColumnDef[] }
  | { kind: "dropTable"; schema: string; name: string; cascade: boolean }
  | { kind: "renameTable"; schema: string; name: string; newName: string }
  | { kind: "addColumn"; schema: string; table: string; column: ColumnDef }
  | { kind: "dropColumn"; schema: string; table: string; column: string; cascade: boolean }
  | { kind: "renameColumn"; schema: string; table: string; column: string; newName: string }
  | {
      kind: "alterColumnType";
      schema: string;
      table: string;
      column: string;
      typeName: string;
      /** Solo hace falta cuando el cambio de tipo no es implícito. */
      using: string | null;
    }
  | { kind: "setColumnNotNull"; schema: string; table: string; column: string; notNull: boolean }
  | { kind: "setColumnDefault"; schema: string; table: string; column: string; default: string | null }
  | { kind: "addConstraint"; schema: string; table: string; name: string; definition: ConstraintDef }
  | { kind: "dropConstraint"; schema: string; table: string; name: string; cascade: boolean };

/** El DDL no admite parámetros: a diferencia de `PreviewStatement`, el texto ya está completo. */
export interface DdlStatement {
  sql: string;
}

export const ddlPreview = (changes: TableChange[]) =>
  invoke<DdlStatement[]>("ddl_preview", { changes });

export const ddlApply = (id: string, changes: TableChange[], database?: string) =>
  invoke<void>("ddl_apply", { id, changes, database: database ?? null });

/** Una constraint tal como ya existe. Solo se puede borrar: no hay "editarla" (ver `ConstraintDef`). */
export interface ConstraintInfo {
  oid: number;
  name: string;
  /** Etiqueta para mostrar ("primaria", "foránea", ...). */
  kind: string;
  definition: string;
}

export const tableConstraints = (id: string, oid: number, database?: string) =>
  invoke<ConstraintInfo[]>("table_constraints", { id, oid, database: database ?? null });

export interface IndexDef {
  schema: string;
  table: string;
  /** Si se deja vacío, Postgres lo nombra solo. */
  name: string | null;
  unique: boolean;
  /** `btree` si se deja vacío. */
  method: string | null;
  columns: string[];
  /** Predicado crudo de un índice parcial. */
  whereClause: string | null;
  /** No bloquea la tabla mientras se construye; no se puede combinar con un lote transaccional. */
  concurrently: boolean;
}

export interface IndexInfo {
  oid: number;
  name: string;
  definition: string;
  primary: boolean;
  unique: boolean;
  /** `false` si quedó de un CREATE INDEX CONCURRENTLY que falló. */
  valid: boolean;
  method: string;
}

export const indexPreview = (def: IndexDef) => invoke<DdlStatement>("index_preview", { def });

/**
 * Lanza la creación del índice y devuelve el identificador de la tarea: no espera a que termine.
 * Con `CONCURRENTLY` sobre una tabla grande, esperar sería tener la ventana tomada durante horas
 * (ver `tasks.svelte.ts`).
 */
export const indexCreate = (
  id: string,
  def: IndexDef,
  channel: Channel<TaskEvent>,
  database?: string,
) => invoke<string>("index_create", { id, def, channel, database: database ?? null });

export const indexDrop = (
  id: string,
  schema: string,
  name: string,
  cascade: boolean,
  concurrently: boolean,
  database?: string,
) => invoke<void>("index_drop", { id, schema, name, cascade, concurrently, database: database ?? null });

export const tableIndexes = (id: string, oid: number, database?: string) =>
  invoke<IndexInfo[]>("table_indexes", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Vistas
// ---------------------------------------------------------------------------

export type ViewChange =
  | {
      kind: "createView";
      schema: string;
      name: string;
      columns: string[];
      query: string;
      /** `CREATE OR REPLACE VIEW` en vez de `CREATE VIEW`. */
      replace: boolean;
    }
  | { kind: "dropView"; schema: string; name: string; cascade: boolean }
  | {
      kind: "createMaterializedView";
      schema: string;
      name: string;
      columns: string[];
      query: string;
      /** `false` deja la vista vacía hasta el próximo refresh (`WITH NO DATA`). */
      withData: boolean;
    }
  | { kind: "dropMaterializedView"; schema: string; name: string; cascade: boolean }
  | {
      kind: "refreshMaterializedView";
      schema: string;
      name: string;
      /** No bloquea a los lectores mientras se refresca; necesita un índice único. */
      concurrently: boolean;
    };

export const viewPreview = (changes: ViewChange[]) =>
  invoke<DdlStatement[]>("view_preview", { changes });

export const viewApply = (id: string, changes: ViewChange[], database?: string) =>
  invoke<void>("view_apply", { id, changes, database: database ?? null });

/** El cuerpo del SELECT, sin el `CREATE VIEW ... AS` alrededor: para precargar el editor. */
export const viewQuery = (id: string, oid: number, database?: string) =>
  invoke<string>("view_query", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Secuencias
// ---------------------------------------------------------------------------

/** La columna que posee la secuencia: al borrarse la columna se borra la secuencia con ella. */
export interface OwnedBy {
  schema: string;
  table: string;
  column: string;
}

/**
 * A qué columna se ata la secuencia, o `none` para desatarla.
 *
 * Es una unión etiquetada y no un `OwnedBy | null` porque del lado de Rust un `null` sería
 * indistinguible de «no lo toques».
 */
export type SequenceOwner =
  | { kind: "none" }
  | { kind: "column"; schema: string; table: string; column: string };

/**
 * Los parámetros de una secuencia. `null` en cualquiera es «no lo toques», no «poné el valor por
 * omisión»: en un `ALTER` la diferencia es todo.
 */
export interface SequenceOptions {
  /** `smallint`, `integer` o `bigint`. Va crudo. */
  dataType?: string | null;
  increment?: number | null;
  minValue?: number | null;
  maxValue?: number | null;
  start?: number | null;
  cache?: number | null;
  cycle?: boolean | null;
  /** `null` no la toca. */
  ownedBy?: SequenceOwner | null;
}

export type SequenceChange =
  | {
      kind: "createSequence";
      schema: string;
      name: string;
      ifNotExists: boolean;
      options: SequenceOptions;
    }
  | { kind: "alterSequence"; schema: string; name: string; options: SequenceOptions }
  /** Mueve la secuencia ahora. `START WITH` dice a dónde vuelve; esto la mueve de verdad. */
  | { kind: "restartSequence"; schema: string; name: string; value: number | null }
  | { kind: "renameSequence"; schema: string; name: string; newName: string }
  | { kind: "setSequenceSchema"; schema: string; name: string; newSchema: string }
  | { kind: "setSequenceOwner"; schema: string; name: string; owner: string }
  | { kind: "dropSequence"; schema: string; name: string; cascade: boolean };

export interface SequenceInfo {
  schema: string;
  name: string;
  owner: string;
  dataType: string;
  start: number;
  increment: number;
  minValue: number;
  maxValue: number;
  cache: number;
  cycle: boolean;
  /** `null` si todavía no se usó, y también si el rol no puede leerla: el servidor no distingue. */
  lastValue: number | null;
  ownedBy: OwnedBy | null;
  comment: string | null;
}

export const sequencePreview = (changes: SequenceChange[]) =>
  invoke<DdlStatement[]>("sequence_preview", { changes });

export const sequenceApply = (id: string, changes: SequenceChange[], database?: string) =>
  invoke<void>("sequence_apply", { id, changes, database: database ?? null });

export const sequenceInfo = (id: string, oid: number, database?: string) =>
  invoke<SequenceInfo>("sequence_info", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Tipos y dominios
// ---------------------------------------------------------------------------

/** Un campo de un tipo compuesto. El tipo va crudo, como el de una columna. */
export interface TypeField {
  name: string;
  dataType: string;
  collation?: string | null;
}

/** Dónde entra un valor nuevo de una enumeración. Sin posición va al final. */
export type EnumPosition = { kind: "before"; value: string } | { kind: "after"; value: string };

export type TypeChange =
  | { kind: "createEnum"; schema: string; name: string; labels: string[] }
  /** No hay `DROP VALUE`: sacar un valor exigiría recrear el tipo y todo lo que lo usa. */
  | {
      kind: "addEnumValue";
      schema: string;
      name: string;
      value: string;
      position: EnumPosition | null;
      ifNotExists: boolean;
    }
  | { kind: "renameEnumValue"; schema: string; name: string; from: string; to: string }
  | { kind: "createComposite"; schema: string; name: string; fields: TypeField[] }
  | { kind: "addCompositeField"; schema: string; name: string; field: TypeField }
  | { kind: "dropCompositeField"; schema: string; name: string; field: string; cascade: boolean }
  | {
      kind: "alterCompositeFieldType";
      schema: string;
      name: string;
      field: string;
      dataType: string;
      collation: string | null;
      cascade: boolean;
    }
  | { kind: "renameType"; schema: string; name: string; newName: string }
  | { kind: "setTypeSchema"; schema: string; name: string; newSchema: string }
  | { kind: "setTypeOwner"; schema: string; name: string; owner: string }
  | { kind: "dropType"; schema: string; name: string; cascade: boolean };

export type TypeKind = "enum" | "composite" | "domain" | "other";

export interface TypeInfo {
  schema: string;
  name: string;
  owner: string;
  kind: TypeKind;
  labels: string[];
  fields: TypeField[];
  comment: string | null;
}

export const typePreview = (changes: TypeChange[]) =>
  invoke<DdlStatement[]>("type_preview", { changes });

export const typeApply = (id: string, changes: TypeChange[], database?: string) =>
  invoke<void>("type_apply", { id, changes, database: database ?? null });

export const typeInfo = (id: string, oid: number, database?: string) =>
  invoke<TypeInfo>("type_info", { id, oid, database: database ?? null });

/** Una restricción `CHECK` de un dominio. `VALUE` es el valor que se está validando. */
export interface DomainConstraint {
  /** Vacío deja que el servidor lo nombre. */
  name: string | null;
  /** La expresión, cruda. */
  check: string;
  /** `NOT VALID`: no revisa lo que ya está guardado. */
  notValid: boolean;
}

export type DomainChange =
  | {
      kind: "createDomain";
      schema: string;
      name: string;
      dataType: string;
      collation: string | null;
      default: string | null;
      notNull: boolean;
      constraints: DomainConstraint[];
    }
  | { kind: "setDomainDefault"; schema: string; name: string; default: string | null }
  | { kind: "setDomainNotNull"; schema: string; name: string; notNull: boolean }
  | { kind: "addDomainConstraint"; schema: string; name: string; constraint: DomainConstraint }
  | { kind: "validateDomainConstraint"; schema: string; name: string; constraint: string }
  | {
      kind: "dropDomainConstraint";
      schema: string;
      name: string;
      constraint: string;
      ifExists: boolean;
      cascade: boolean;
    }
  | { kind: "renameDomain"; schema: string; name: string; newName: string }
  | { kind: "setDomainSchema"; schema: string; name: string; newSchema: string }
  | { kind: "setDomainOwner"; schema: string; name: string; owner: string }
  | { kind: "dropDomain"; schema: string; name: string; cascade: boolean };

export interface DomainInfo {
  schema: string;
  name: string;
  owner: string;
  dataType: string;
  collation: string | null;
  default: string | null;
  notNull: boolean;
  constraints: DomainConstraint[];
  comment: string | null;
}

export const domainPreview = (changes: DomainChange[]) =>
  invoke<DdlStatement[]>("domain_preview", { changes });

export const domainApply = (id: string, changes: DomainChange[], database?: string) =>
  invoke<void>("domain_apply", { id, changes, database: database ?? null });

export const domainInfo = (id: string, oid: number, database?: string) =>
  invoke<DomainInfo>("domain_info", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Esquemas y bases
// ---------------------------------------------------------------------------

export type SchemaChange =
  | { kind: "createSchema"; name: string; authorization: string | null; ifNotExists: boolean }
  | { kind: "renameSchema"; name: string; newName: string }
  | { kind: "setSchemaOwner"; name: string; owner: string }
  /** Sin `CASCADE` falla si tiene algo adentro, a propósito. */
  | { kind: "dropSchema"; name: string; ifExists: boolean; cascade: boolean };

export const schemaPreview = (changes: SchemaChange[]) =>
  invoke<DdlStatement[]>("schema_preview", { changes });

export const schemaApply = (id: string, changes: SchemaChange[], database?: string) =>
  invoke<void>("schema_apply", { id, changes, database: database ?? null });

/** Lo que se puede pedir al crear una base. Cada campo vacío lo decide el servidor. */
export interface DatabaseOptions {
  owner?: string | null;
  template?: string | null;
  encoding?: string | null;
  lcCollate?: string | null;
  lcCtype?: string | null;
  tablespace?: string | null;
  /** `-1` es sin límite. */
  connectionLimit?: number | null;
  isTemplate?: boolean | null;
}

export type DatabaseChange =
  | { kind: "createDatabase"; name: string; options: DatabaseOptions }
  | { kind: "renameDatabase"; name: string; newName: string }
  | { kind: "setDatabaseOwner"; name: string; owner: string }
  | { kind: "setDatabaseConnectionLimit"; name: string; limit: number }
  | { kind: "setDatabaseAllowConnections"; name: string; allow: boolean }
  /** `force` echa a las sesiones conectadas en vez de fallar. */
  | { kind: "dropDatabase"; name: string; ifExists: boolean; force: boolean };

export interface DatabaseInfo {
  name: string;
  owner: string;
  encoding: string;
  collate: string;
  ctype: string;
  tablespace: string;
  connectionLimit: number;
  allowConnections: boolean;
  isTemplate: boolean;
  /** Bytes. `null` si el rol no puede leerlo. */
  size: number | null;
  comment: string | null;
}

export const databasePreview = (changes: DatabaseChange[]) =>
  invoke<DdlStatement[]>("database_preview", { changes });

/**
 * Aplica los cambios **sin** transacción: `CREATE DATABASE` y `DROP DATABASE` no la admiten. Por eso
 * conviene mandar un cambio por vez: una lista a medias deja hecho lo anterior.
 */
export const databaseApply = (id: string, changes: DatabaseChange[]) =>
  invoke<void>("database_apply", { id, changes });

export const databaseInfo = (id: string, name: string) =>
  invoke<DatabaseInfo>("database_info", { id, name });

// ---------------------------------------------------------------------------
// Particiones
// ---------------------------------------------------------------------------

/** El límite de una partición. Los valores van crudos: admiten `MINVALUE`, `MAXVALUE` y funciones. */
export type PartitionBound =
  | { kind: "range"; from: string[]; to: string[] }
  | { kind: "list"; values: string[] }
  | { kind: "hash"; modulus: number; remainder: number }
  /** Se lleva todo lo que no entra en ninguna otra. */
  | { kind: "default" };

export type PartitionChange =
  | {
      kind: "createPartition";
      parentSchema: string;
      parent: string;
      schema: string;
      name: string;
      bound: PartitionBound;
      /** Cuando la partición es a su vez particionada: `RANGE (dia)`, crudo. */
      partitionBy: string | null;
    }
  /** El servidor revisa que ninguna fila de la tabla se salga del límite. */
  | {
      kind: "attachPartition";
      parentSchema: string;
      parent: string;
      schema: string;
      name: string;
      bound: PartitionBound;
    }
  | {
      kind: "detachPartition";
      parentSchema: string;
      parent: string;
      schema: string;
      name: string;
      /** Sin bloquear a los lectores. Pide PostgreSQL 14 o más. */
      concurrently: boolean;
      /** Termina un `DETACH … CONCURRENTLY` que quedó a medias. */
      finalize: boolean;
    }
  | { kind: "dropPartition"; schema: string; name: string; cascade: boolean };

export interface PartitionInfo {
  schema: string;
  name: string;
  /** El límite tal como lo escribe el servidor. */
  bound: string;
  partitioned: boolean;
}

export interface PartitioningInfo {
  /** La estrategia tal como la escribe el servidor: `RANGE (creado)`, `LIST (region)`, … */
  strategy: string;
  partitions: PartitionInfo[];
}

/** Pide el perfil, a diferencia de las demás vistas previas: `CONCURRENTLY` depende de la versión. */
export const partitionPreview = (id: string, changes: PartitionChange[]) =>
  invoke<DdlStatement[]>("partition_preview", { id, changes });

export const partitionApply = (id: string, changes: PartitionChange[], database?: string) =>
  invoke<void>("partition_apply", { id, changes, database: database ?? null });

export const tablePartitions = (id: string, oid: number, database?: string) =>
  invoke<PartitioningInfo>("table_partitions", { id, oid, database: database ?? null });
