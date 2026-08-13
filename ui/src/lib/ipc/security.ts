import { invoke } from "./core";

import type { DdlStatement } from "./ddl";

// ---------------------------------------------------------------------------
// Privilegios
// ---------------------------------------------------------------------------

export type TablePrivilege =
  | "select"
  | "insert"
  | "update"
  | "delete"
  | "truncate"
  | "references"
  | "trigger";

export type ColumnPrivilege = "select" | "insert" | "update" | "references";

export type SchemaPrivilege = "usage" | "create";

export type SequencePrivilege = "usage" | "select" | "update";

export type FunctionPrivilege = "execute";

export type DatabasePrivilege = "connect" | "create" | "temporary";

export type TypePrivilege = "usage";

/**
 * Sobre qué se otorga o se revoca. El objeto y su vocabulario van juntos a propósito: así el tipo
 * no deja pedir `TRUNCATE` sobre un esquema.
 */
export type Grantable =
  | { on: "table"; schema: string; table: string; privileges: TablePrivilege[] }
  | {
      on: "columns";
      schema: string;
      table: string;
      columns: string[];
      privileges: ColumnPrivilege[];
    }
  | { on: "schema"; schema: string; privileges: SchemaPrivilege[] }
  | { on: "sequence"; schema: string; sequence: string; privileges: SequencePrivilege[] }
  | {
      on: "function";
      schema: string;
      name: string;
      /** Ya formateados por el servidor (`functionArgs`); no se reconstruyen acá. */
      args: string;
      procedure: boolean;
      privileges: FunctionPrivilege[];
    }
  | { on: "database"; database: string; privileges: DatabasePrivilege[] };

/** Sobre qué actúan los privilegios por omisión. */
export type DefaultPrivileges =
  | { on: "tables"; privileges: TablePrivilege[] }
  | { on: "sequences"; privileges: SequencePrivilege[] }
  | { on: "functions"; privileges: FunctionPrivilege[] }
  | { on: "types"; privileges: TypePrivilege[] };

/**
 * Cuándo se aplican los privilegios por omisión: solo a lo que cree `role` (por omisión, quien
 * ejecuta la sentencia) dentro de `schema` (por omisión, cualquiera).
 */
export interface DefaultScope {
  role?: string | null;
  schema?: string | null;
}

export type PrivilegeChange =
  | { kind: "grant"; target: Grantable; grantee: string; grantOption: boolean }
  | {
      kind: "revoke";
      target: Grantable;
      grantee: string;
      /** `REVOKE GRANT OPTION FOR ...`: revoca solo el permiso de volver a otorgar. */
      grantOptionOnly: boolean;
      cascade: boolean;
    }
  | {
      kind: "grantDefault";
      scope: DefaultScope;
      target: DefaultPrivileges;
      grantee: string;
      grantOption: boolean;
    }
  | {
      kind: "revokeDefault";
      scope: DefaultScope;
      target: DefaultPrivileges;
      grantee: string;
      grantOptionOnly: boolean;
      cascade: boolean;
    };

/** Un privilegio ya otorgado, tal como sale de `aclexplode`. */
export interface PrivilegeGrant {
  /** Nombre del rol, o `"PUBLIC"`. */
  grantee: string;
  /** Tal como lo devuelve el servidor: `"SELECT"`, `"INSERT"`, ... */
  privilege: string;
  grantable: boolean;
}

/** Un privilegio otorgado sobre una columna suelta. */
export interface ColumnGrant extends PrivilegeGrant {
  column: string;
}

/** Un privilegio por omisión ya definido. */
export interface DefaultGrant extends PrivilegeGrant {
  /** El rol cuyas creaciones futuras dispararán el privilegio. */
  owner: string;
  /** El esquema donde vale, o `null` si vale en todos. */
  schema: string | null;
  /** `"tables"`, `"sequences"`, `"functions"` o `"types"`. */
  objects: string;
}

export const privilegePreview = (changes: PrivilegeChange[]) =>
  invoke<DdlStatement[]>("privilege_preview", { changes });

export const privilegeApply = (id: string, changes: PrivilegeChange[], database?: string) =>
  invoke<void>("privilege_apply", { id, changes, database: database ?? null });

/** Sirve para todo lo que vive en `pg_class`: tablas, vistas, vistas materializadas y secuencias. */
export const relationPrivileges = (id: string, oid: number, database?: string) =>
  invoke<PrivilegeGrant[]>("relation_privileges", { id, oid, database: database ?? null });

export const functionPrivileges = (id: string, oid: number, database?: string) =>
  invoke<PrivilegeGrant[]>("function_privileges", { id, oid, database: database ?? null });

export const databasePrivileges = (id: string, name: string, database?: string) =>
  invoke<PrivilegeGrant[]>("database_privileges", { id, name, database: database ?? null });

export const columnPrivileges = (id: string, oid: number, database?: string) =>
  invoke<ColumnGrant[]>("column_privileges", { id, oid, database: database ?? null });

export const defaultPrivileges = (id: string, database?: string) =>
  invoke<DefaultGrant[]>("default_privileges", { id, database: database ?? null });

export const schemaPrivileges = (id: string, oid: number, database?: string) =>
  invoke<PrivilegeGrant[]>("schema_privileges", { id, oid, database: database ?? null });

// ---------------------------------------------------------------------------
// Seguridad por fila (RLS)
// ---------------------------------------------------------------------------

export type PolicyCommand = "all" | "select" | "insert" | "update" | "delete";

/** `permissive` suma permisos (se combinan con OR); `restrictive` los recorta (con AND). */
export type PolicyKind = "permissive" | "restrictive";

export interface PolicyDef {
  command: PolicyCommand;
  kind: PolicyKind;
  /** Vacío significa PUBLIC. */
  roles: string[];
  /** Expresión SQL cruda: qué filas se ven. No vale para INSERT. */
  using: string | null;
  /** Expresión SQL cruda: qué filas se pueden escribir. No vale para SELECT ni DELETE. */
  check: string | null;
}

/** No hay «editar»: cambiar una política es borrarla y crearla de nuevo, como un trigger. */
export type PolicyChange =
  | { kind: "createPolicy"; schema: string; table: string; name: string; definition: PolicyDef }
  | { kind: "dropPolicy"; schema: string; table: string; name: string }
  /** El interruptor de la tabla. Sin esto las políticas no se aplican. */
  | { kind: "setRowSecurity"; schema: string; table: string; enabled: boolean }
  /** Que el filtro alcance también al dueño de la tabla. */
  | { kind: "setForceRowSecurity"; schema: string; table: string; forced: boolean };

export interface PolicyInfo {
  oid: number;
  name: string;
  command: PolicyCommand;
  kind: PolicyKind;
  /** Vacío significa PUBLIC. */
  roles: string[];
  using: string | null;
  check: string | null;
}

export interface TableSecurity {
  enabled: boolean;
  forced: boolean;
  policies: PolicyInfo[];
}

export const policyPreview = (changes: PolicyChange[]) =>
  invoke<DdlStatement[]>("policy_preview", { changes });

export const policyApply = (id: string, changes: PolicyChange[], database?: string) =>
  invoke<void>("policy_apply", { id, changes, database: database ?? null });

export const tableSecurity = (id: string, oid: number, database?: string) =>
  invoke<TableSecurity>("table_security", { id, oid, database: database ?? null });