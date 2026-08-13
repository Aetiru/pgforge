/**
 * El resto de los objetos con vista previa: comentarios, funciones, disparadores, roles,
 * extensiones y datos externos.
 */

import { invoke } from "@tauri-apps/api/core";

import type { DdlStatement } from "./ddl";

// ---------------------------------------------------------------------------
// Comentarios
// ---------------------------------------------------------------------------

/** Sobre qué objeto se comenta. La lista es la de los nodos que muestra el árbol. */
export type CommentTarget =
  | { kind: "table"; schema: string; name: string }
  | { kind: "column"; schema: string; table: string; column: string }
  | { kind: "view"; schema: string; name: string }
  | { kind: "materializedView"; schema: string; name: string }
  | { kind: "foreignTable"; schema: string; name: string }
  | { kind: "sequence"; schema: string; name: string }
  | { kind: "index"; schema: string; name: string }
  | { kind: "type"; schema: string; name: string }
  | { kind: "domain"; schema: string; name: string }
  | { kind: "schema"; name: string }
  | { kind: "database"; name: string }
  | { kind: "role"; name: string }
  | { kind: "extension"; name: string }
  /** La firma hace falta para distinguir entre sobrecargas. */
  | { kind: "function"; schema: string; name: string; arguments: string }
  | { kind: "procedure"; schema: string; name: string; arguments: string }
  | { kind: "trigger"; schema: string; table: string; name: string }
  | { kind: "constraint"; schema: string; table: string; name: string }
  | { kind: "policy"; schema: string; table: string; name: string };

export interface CommentChange {
  target: CommentTarget;
  /** `null` —o en blanco— borra el comentario: vacío no es lo mismo que ninguno. */
  comment: string | null;
}

export const commentPreview = (changes: CommentChange[]) =>
  invoke<DdlStatement[]>("comment_preview", { changes });

export const commentApply = (id: string, changes: CommentChange[], database?: string) =>
  invoke<void>("comment_apply", { id, changes, database: database ?? null });

// ---------------------------------------------------------------------------
// Funciones y procedimientos
// ---------------------------------------------------------------------------

/** Ejecuta la sentencia `CREATE [OR REPLACE] FUNCTION`/`PROCEDURE` tal cual la escribió el usuario. */
export const functionApply = (id: string, sql: string, database?: string) =>
  invoke<void>("function_apply", { id, sql, database: database ?? null });

export const functionDrop = (
  id: string,
  schema: string,
  name: string,
  args: string,
  procedure: boolean,
  cascade: boolean,
  database?: string,
) =>
  invoke<void>("function_drop", {
    id,
    schema,
    name,
    args,
    procedure,
    cascade,
    database: database ?? null,
  });

/** La lista de tipos de argumento, para armar el `DROP FUNCTION`/`DROP PROCEDURE`. */
export const functionArgs = (id: string, oid: number, database?: string) =>
  invoke<string>("function_args", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Triggers
// ---------------------------------------------------------------------------

export type Timing = "before" | "after" | "insteadOf";
/** No se llama `Event`: choca con el tipo `Event` del DOM. */
export type TriggerEvent = "insert" | "update" | "delete" | "truncate";
export type TriggerLevel = "row" | "statement";

export interface TriggerDef {
  timing: Timing;
  events: TriggerEvent[];
  level: TriggerLevel;
  when: string | null;
  functionSchema: string;
  functionName: string;
}

export type TriggerChange =
  | { kind: "createTrigger"; schema: string; table: string; name: string; definition: TriggerDef }
  | { kind: "dropTrigger"; schema: string; table: string; name: string; cascade: boolean };

export interface TriggerInfo {
  oid: number;
  name: string;
  timing: Timing;
  events: TriggerEvent[];
  level: TriggerLevel;
  when: string | null;
  functionSchema: string;
  functionName: string;
}

export const triggerPreview = (changes: TriggerChange[]) =>
  invoke<DdlStatement[]>("trigger_preview", { changes });

export const triggerApply = (id: string, changes: TriggerChange[], database?: string) =>
  invoke<void>("trigger_apply", { id, changes, database: database ?? null });

export const tableTriggers = (id: string, oid: number, database?: string) =>
  invoke<TriggerInfo[]>("table_triggers", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

/** `undefined`/ausente en un campo significa "no tocar": para crear se manda todo, para editar solo lo que cambió. */
export interface RoleAttributes {
  superuser?: boolean;
  createdb?: boolean;
  createrole?: boolean;
  inherit?: boolean;
  login?: boolean;
  replication?: boolean;
  bypassRls?: boolean;
  connectionLimit?: number;
  /** `undefined` en edición: Postgres nunca devuelve la contraseña, así que no hay con qué precargarla. */
  password?: string;
  validUntil?: string;
}

export type RoleChange =
  | { kind: "createRole"; name: string; attributes: RoleAttributes; memberOf: string[] }
  | { kind: "alterRole"; name: string; attributes: RoleAttributes }
  | { kind: "renameRole"; name: string; newName: string }
  | { kind: "dropRole"; name: string }
  /** Solo alcanza a la base conectada: hay que repetirlo en cada base donde el rol tenga algo. */
  | { kind: "reassignOwned"; from: string; to: string }
  | { kind: "dropOwned"; role: string; cascade: boolean }
  | { kind: "grantMembership"; role: string; member: string; adminOption: boolean }
  | { kind: "revokeMembership"; role: string; member: string };

export interface RoleInfo {
  oid: number;
  name: string;
  superuser: boolean;
  createdb: boolean;
  createrole: boolean;
  inherit: boolean;
  login: boolean;
  replication: boolean;
  bypassRls: boolean;
  connectionLimit: number;
  validUntil: string | null;
}

export const rolePreview = (changes: RoleChange[]) =>
  invoke<DdlStatement[]>("role_preview", { changes });

export const roleApply = (id: string, changes: RoleChange[], database?: string) =>
  invoke<void>("role_apply", { id, changes, database: database ?? null });

export const roleInfo = (id: string, oid: number, database?: string) =>
  invoke<RoleInfo>("role_info", { id, oid, database: database ?? null });

export const roleMemberships = (id: string, name: string, database?: string) =>
  invoke<string[]>("role_memberships", { id, name, database: database ?? null });

// ---------------------------------------------------------------------------
// Extensiones
// ---------------------------------------------------------------------------

export type ExtensionChange =
  | {
      kind: "create";
      name: string;
      schema: string | null;
      version: string | null;
      cascade: boolean;
    }
  | { kind: "update"; name: string; version: string | null }
  | { kind: "setSchema"; name: string; schema: string }
  | { kind: "drop"; name: string; cascade: boolean };

export interface ExtensionInfo {
  name: string;
  version: string;
  schema: string;
  comment: string | null;
  /** Solo si es relocatable tiene sentido cambiarla de esquema. */
  relocatable: boolean;
  /** Puede ser más nueva que `version`: entonces hay una actualización disponible. */
  defaultVersion: string | null;
  availableVersions: string[];
}

export interface AvailableExtension {
  name: string;
  defaultVersion: string | null;
  installed: boolean;
  comment: string | null;
}

export const extensionPreview = (changes: ExtensionChange[]) =>
  invoke<DdlStatement[]>("extension_preview", { changes });

export const extensionApply = (id: string, changes: ExtensionChange[], database?: string) =>
  invoke<void>("extension_apply", { id, changes, database: database ?? null });

export const extensionInfo = (id: string, name: string, database?: string) =>
  invoke<ExtensionInfo>("extension_info", { id, name, database: database ?? null });

export const availableExtensions = (id: string, database?: string) =>
  invoke<AvailableExtension[]>("available_extensions", { id, database: database ?? null });

// ---------------------------------------------------------------------------
// Datos externos (wrappers, servidores foráneos, mapeos de usuario)
// ---------------------------------------------------------------------------

/** Una opción como par [clave, valor]: espeja el `(String, String)` del núcleo. */
export type FdwOption = [string, string];

/** Qué cambia de la lista de opciones: altas, cambios de valor y bajas. */
export interface OptionsDelta {
  add: FdwOption[];
  set: FdwOption[];
  drop: string[];
}

export type FdwChange =
  | {
      kind: "create";
      name: string;
      handler: string | null;
      validator: string | null;
      options: FdwOption[];
    }
  | {
      kind: "alter";
      name: string;
      handler: string | null;
      noHandler: boolean;
      validator: string | null;
      noValidator: boolean;
      options: OptionsDelta;
    }
  | { kind: "drop"; name: string; cascade: boolean };

export interface FdwInfo {
  name: string;
  handler: string | null;
  validator: string | null;
  options: FdwOption[];
  owner: string;
}

export type ServerChange =
  | {
      kind: "create";
      name: string;
      fdw: string;
      serverType: string | null;
      version: string | null;
      options: FdwOption[];
    }
  | { kind: "alter"; name: string; version: string | null; options: OptionsDelta }
  | { kind: "drop"; name: string; cascade: boolean };

export interface ServerInfo {
  name: string;
  fdw: string;
  serverType: string | null;
  version: string | null;
  options: FdwOption[];
  owner: string;
}

export type UserMappingChange =
  | { kind: "create"; server: string; user: string; options: FdwOption[] }
  | { kind: "alter"; server: string; user: string; options: OptionsDelta }
  | { kind: "drop"; server: string; user: string };

export interface UserMapping {
  user: string;
  /** `null` cuando el rol conectado no puede ver las opciones del mapeo. */
  options: FdwOption[] | null;
}

export const fdwPreview = (changes: FdwChange[]) =>
  invoke<DdlStatement[]>("fdw_preview", { changes });

export const fdwApply = (id: string, changes: FdwChange[], database?: string) =>
  invoke<void>("fdw_apply", { id, changes, database: database ?? null });

export const fdwInfo = (id: string, name: string, database?: string) =>
  invoke<FdwInfo>("fdw_info", { id, name, database: database ?? null });

export const availableFdws = (id: string, database?: string) =>
  invoke<string[]>("available_fdws", { id, database: database ?? null });

export const foreignServerPreview = (changes: ServerChange[]) =>
  invoke<DdlStatement[]>("foreign_server_preview", { changes });

export const foreignServerApply = (id: string, changes: ServerChange[], database?: string) =>
  invoke<void>("foreign_server_apply", { id, changes, database: database ?? null });

export const foreignServerInfo = (id: string, name: string, database?: string) =>
  invoke<ServerInfo>("foreign_server_info", { id, name, database: database ?? null });

export const userMappingPreview = (changes: UserMappingChange[]) =>
  invoke<DdlStatement[]>("user_mapping_preview", { changes });

export const userMappingApply = (id: string, changes: UserMappingChange[], database?: string) =>
  invoke<void>("user_mapping_apply", { id, changes, database: database ?? null });

export const userMappings = (id: string, server: string, database?: string) =>
  invoke<UserMapping[]>("user_mappings", { id, server, database: database ?? null });
