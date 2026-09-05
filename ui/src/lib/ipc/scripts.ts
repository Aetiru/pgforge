/**
 * Scripts: los `.sql` de cada conexión, guardados solos en una carpeta por conexión.
 *
 * Espejo de `pgforge_core::scripts`. Toda ruta que llega acá pasa, del lado de Rust, por la guarda
 * `within()` antes de tocar disco: ningún comando de este módulo la repite.
 */

import { invoke } from "./core";

export interface ScriptEntry {
  name: string;
  path: string;
}

/** Una carpeta del árbol de scripts: en el primer nivel, una por conexión. */
export interface ScriptFolder {
  name: string;
  path: string;
  files: ScriptEntry[];
  folders: ScriptFolder[];
}

export interface ImportReport {
  imported: number;
  /** Motivo textual de lo que falló al copiar; lo que entró con otro nombre por chocar no cuenta acá. */
  skipped: string[];
}

export const scriptsTree = () => invoke<ScriptFolder[]>("scripts_tree");

/** Dónde está la carpeta raíz de scripts, para el botón «Abrir la carpeta». */
export const scriptsRootPath = () => invoke<string>("scripts_root_path");

/**
 * La ruta completa (ya armada del lado de Rust, con el nombre de la conexión saneado) del próximo
 * `scriptN.sql` libre para esa conexión.
 */
export const scriptNewName = (profileId: string) =>
  invoke<string>("script_new_name", { profileId });

export const scriptRead = (path: string) => invoke<string>("script_read", { path });

export const scriptWrite = (path: string, content: string) =>
  invoke<void>("script_write", { path, content });

export const scriptRename = (old: string, next: string) =>
  invoke<void>("script_rename", { old, new: next });

export const scriptDelete = (path: string) => invoke<void>("script_delete", { path });

export const scriptCreateFolder = (path: string) => invoke<void>("script_create_folder", { path });

/** Importa los `.sql` de una carpeta elegida por el usuario hacia la carpeta de una conexión. */
export const scriptsImportFolder = (profileId: string, src: string) =>
  invoke<ImportReport>("scripts_import_folder", { profileId, src });
