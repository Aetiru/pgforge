import { invoke } from "@tauri-apps/api/core";

/** Error del núcleo tal como cruza el IPC. Refleja `pgforge_core::error::ErrorPayload`. */
export type CoreError =
  | { kind: "canceled" }
  | { kind: "permission"; message: string }
  | {
      kind: "database";
      code: string;
      message: string;
      detail: string | null;
      hint: string | null;
      position: number | null;
    }
  | { kind: "other"; message: string };

export type SslMode = "disable" | "prefer" | "require" | "verifyCa" | "verifyFull";

export interface SshTunnel {
  host: string;
  port: number;
  user: string;
  privateKey?: string;
}

export interface ConnectionProfile {
  id: string;
  name: string;
  group?: string;
  host: string;
  port: number;
  database: string;
  user: string;
  sslMode: SslMode;
  rootCert?: string;
  connectTimeoutSecs: number;
  statementTimeoutMs?: number;
  tunnel?: SshTunnel;
  savePassword: boolean;
}

export interface ServerCaps {
  /** `server_version_num`: mayor * 10000 + menor. */
  version: number;
  currentUser: string;
  currentDatabase: string;
  isSuperuser: boolean;
  canSignalBackends: boolean;
  canReadAllStats: boolean;
}

export type FolderKind =
  | "schemas"
  | "tables"
  | "views"
  | "materializedViews"
  | "foreignTables"
  | "sequences"
  | "functions"
  | "procedures"
  | "types"
  | "columns"
  | "indexes"
  | "constraints"
  | "triggers";

export type NodeKind =
  | "database"
  | "schema"
  | "table"
  | "partitionedTable"
  | "foreignTable"
  | "view"
  | "materializedView"
  | "sequence"
  | "function"
  | "procedure"
  | "type"
  | "column"
  | "index"
  | "constraint"
  | "trigger"
  | { folder: FolderKind };

export interface TreeNode {
  id: string;
  label: string;
  detail?: string;
  kind: NodeKind;
  hasChildren: boolean;
  database: string;
  schema?: string;
  oid?: number;
  comment?: string;
}

export interface TreeOptions {
  showSystemSchemas: boolean;
}

export interface Ddl {
  sql: string;
  source: "catalog" | "pgDump";
}

export interface AppInfo {
  version: string;
  minPostgresMajor: number;
}

export interface Connected {
  profile: ConnectionProfile;
  caps: ServerCaps;
}

export const appInfo = () => invoke<AppInfo>("app_info");

export const listProfiles = () => invoke<ConnectionProfile[]>("list_profiles");

export const saveProfile = (profile: ConnectionProfile, password?: string) =>
  invoke<ConnectionProfile>("save_profile", { profile, password: password || null });

export const deleteProfile = (id: string) => invoke<void>("delete_profile", { id });

export const connect = (id: string, password?: string) =>
  invoke<Connected>("connect", { id, password: password || null });

export const disconnect = (id: string) => invoke<void>("disconnect", { id });

export const connectedServers = () => invoke<string[]>("connected_servers");

export const treeChildren = (id: string, parent: TreeNode | null, options: TreeOptions) =>
  invoke<TreeNode[]>("tree_children", { id, parent, options });

export const objectDdl = (id: string, node: TreeNode) =>
  invoke<Ddl>("object_ddl", { id, node });

/** `160004` se muestra como `16.4`. */
export function formatVersion(versionNum: number): string {
  return `${Math.floor(versionNum / 10000)}.${versionNum % 10000}`;
}

export function folderOf(kind: NodeKind): FolderKind | null {
  return typeof kind === "object" ? kind.folder : null;
}

/** `true` si el error viene de una cancelación pedida por el usuario, que no es una falla. */
export function isCanceled(error: unknown): boolean {
  return typeof error === "object" && error !== null && (error as CoreError).kind === "canceled";
}

/** Texto legible para cualquier error del núcleo. */
export function describeError(error: unknown): string {
  const e = error as CoreError;
  switch (e?.kind) {
    case "canceled":
      return "Operación cancelada.";
    case "permission":
      return `Permiso insuficiente: ${e.message}`;
    case "database":
      return e.hint ? `${e.message} (${e.hint})` : e.message;
    case "other":
      return e.message;
    default:
      return String(error);
  }
}
