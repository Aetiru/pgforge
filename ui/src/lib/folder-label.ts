import { t } from "./i18n.svelte";
import type { FolderKind } from "./ipc/schema";

/**
 * Rótulo de una carpeta del árbol en el idioma de la interfaz.
 *
 * El núcleo entrega el rótulo en español (`Folder::label`) porque sus errores y textos son de
 * un solo idioma; traducirlo aquí, según el tipo de carpeta y no según el texto recibido, evita
 * que un cambio de redacción en Rust deje la carpeta sin traducir sin que nada falle.
 */
export function folderLabel(folder: FolderKind): string {
  switch (folder) {
    case "schemas": return t("Esquemas");
    case "tables": return t("Tablas");
    case "views": return t("Vistas");
    case "materializedViews": return t("Vistas materializadas");
    case "foreignTables": return t("Tablas externas");
    case "sequences": return t("Secuencias");
    case "functions": return t("Funciones");
    case "procedures": return t("Procedimientos");
    case "types": return t("Tipos");
    case "columns": return t("Columnas");
    case "indexes": return t("Índices");
    case "constraints": return t("Restricciones");
    case "triggers": return t("Disparadores");
    case "policies": return t("Políticas");
    case "roles": return t("Roles");
    case "databases": return t("Bases de datos");
    case "extensions": return t("Extensiones");
    case "fdws": return t("Wrappers foráneos");
    case "fservers": return t("Servidores foráneos");
  }
}
