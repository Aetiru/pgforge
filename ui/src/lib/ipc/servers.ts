/**
 * Perfiles de conexión, carpetas y datos de la aplicación: lo que hay antes de estar conectado.
 */

import { invoke } from "./core";

export type SslMode = "disable" | "prefer" | "require" | "verifyCa" | "verifyFull";

/** Para qué se usa el servidor. No cambia cómo se conecta: cambia cuánto se avisa antes de tocarlo. */
export type Environment = "dev" | "test" | "prod";

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
  environment?: Environment;
  /** Abre toda conexión al servidor con `default_transaction_read_only`. */
  readOnly: boolean;
  /** Valor inicial del autocommit de cada pestaña de consulta. */
  autocommit: boolean;
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

export interface AppInfo {
  version: string;
  minPostgresMajor: number;
  /** Dónde quedan los archivos de registro, para poder decírselo al usuario. */
  logDir?: string | null;
}

export interface Connected {
  profile: ConnectionProfile;
  caps: ServerCaps;
}

export const appInfo = () => invoke<AppInfo>("app_info");

export const listProfiles = () => invoke<ConnectionProfile[]>("list_profiles");

export const saveProfile = (profile: ConnectionProfile, password?: string, sshPassword?: string) =>
  invoke<ConnectionProfile>("save_profile", {
    profile,
    password: password || null,
    sshPassword: sshPassword || null,
  });

/** Un servidor encontrado en otra herramienta. Sin contraseña: eso se pide al conectar. */
export interface ImportCandidate {
  origin: "pgpass" | "service" | "dbeaver";
  /** El archivo del que salió. */
  source: string;
  name: string;
  host: string;
  port: number;
  database: string;
  /** Puede venir vacío: DBeaver guarda el usuario junto con la contraseña, cifrado aparte. */
  user: string;
  /** La carpeta que tenía en la otra herramienta. */
  group?: string;
  environment?: Environment;
}

export const importScan = () => invoke<ImportCandidate[]>("import_scan");

export const importApply = (candidates: ImportCandidate[], group?: string) =>
  invoke<ConnectionProfile[]>("import_apply", { candidates, group: group ?? null });

export const deleteProfile = (id: string) => invoke<void>("delete_profile", { id });

/** Las carpetas en las que están repartidos los servidores guardados. */
export const listGroups = () => invoke<string[]>("list_groups");

/** Renombra una carpeta, o la deshace si no se pasa nombre nuevo. Devuelve cuántos se movieron. */
export const renameGroup = (from: string, to?: string) =>
  invoke<number>("rename_group", { from, to: to ?? null });

export const connect = (
  id: string,
  password?: string,
  sshPassword?: string,
  trustHostKey?: boolean,
) =>
  invoke<Connected>("connect", {
    id,
    password: password || null,
    sshPassword: sshPassword || null,
    trustHostKey: trustHostKey ?? null,
  });

/**
 * Prueba el túnel SSH del perfil sin conectar a la base. Devuelve el error `sshHostKey` si la clave
 * del bastión no está verificada, igual que `connect`, para reusar el mismo flujo de confirmación.
 */
export const sshTest = (profile: ConnectionProfile, sshPassword?: string, trustHostKey?: boolean) =>
  invoke<void>("ssh_test", {
    profile,
    sshPassword: sshPassword || null,
    trustHostKey: trustHostKey ?? null,
  });

export const disconnect = (id: string) => invoke<void>("disconnect", { id });

export const connectedServers = () => invoke<string[]>("connected_servers");
