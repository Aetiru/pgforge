/**
 * Migraciones versionadas: el script guardado como `V012__descripcion.sql` en una carpeta elegida,
 * y el registro de lo aplicado contra cada servidor.
 *
 * Espejo de `pgforge_core::migrations`. La carpeta se elige con `pickOpen({ directory: true })` y
 * viaja como `folder`: el comando la verifica contra lo que el usuario eligió en el diálogo nativo,
 * así que una ruta escrita a mano se rechaza. Aplicar es una tarea en segundo plano, como el
 * mantenimiento: devuelve el identificador y el avance se sigue en la vista de procesos.
 */

import { invoke } from "./core";

export type MigrationDirection = "up" | "down";

/** `pending` no se aplicó; `applied` coincide con el archivo; `changed` se aplicó y el archivo cambió. */
export type MigrationState = "pending" | "applied" | "changed" | "failed";

export interface MigrationItem {
  version: number;
  /** El tramo del nombre del archivo, con `_`. */
  description: string;
  state: MigrationState;
  checksum: string;
  appliedChecksum: string | null;
  /** Segundos desde el epoch. */
  appliedAt: number | null;
  hasDown: boolean;
  error: string | null;
  /** Hay una aplicada con número mayor. */
  outOfOrder: boolean;
}

export interface MigrationMissing {
  version: number;
  description: string;
  appliedAt: number;
}

export interface MigrationRecord {
  id: number;
  profileId: string;
  database: string;
  serverName: string | null;
  version: number;
  description: string;
  direction: MigrationDirection;
  checksum: string;
  appliedAt: number;
  seconds: number;
  succeeded: boolean;
  error: string | null;
}

export interface MigrationOverview {
  report: { items: MigrationItem[]; missing: MigrationMissing[] };
  orphanDowns: string[];
  records: MigrationRecord[];
}

export interface MigrationPlan {
  version: number;
  description: string;
  direction: MigrationDirection;
  /** Huella del archivo en la vista previa; se devuelve al aplicar. */
  checksum: string;
  execution: { mode: "atomic" | "selfManaged" | "sequential"; steps: string[] };
  warnings: string[];
}

export interface MigrationSaved {
  version: number;
  upFile: string;
  downFile: string | null;
}

export const migrationSave = (
  folder: string,
  description: string,
  upSql: string,
  downSql: string | null,
) => invoke<MigrationSaved>("migration_save", { folder, description, upSql, downSql });

export const migrationOverview = (id: string, database: string | undefined, folder: string) =>
  invoke<MigrationOverview>("migration_overview", { id, database: database ?? null, folder });

export const migrationPreview = (
  id: string,
  database: string | undefined,
  folder: string,
  version: number,
  direction: MigrationDirection,
) =>
  invoke<MigrationPlan>("migration_preview", {
    id,
    database: database ?? null,
    folder,
    version,
    direction,
  });

/** Lanza la migración como tarea y devuelve su identificador. */
export const migrationApply = (
  id: string,
  database: string | undefined,
  folder: string,
  version: number,
  direction: MigrationDirection,
  expectedChecksum: string,
) =>
  invoke<string>("migration_apply", {
    id,
    database: database ?? null,
    folder,
    version,
    direction,
    expectedChecksum,
  });
