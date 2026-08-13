import { invoke } from "./core";

import type { DdlStatement } from "./ddl";

// ---------------------------------------------------------------------------
// Configuración del servidor (pg_settings)
// ---------------------------------------------------------------------------

/** `bool`, `integer`, `real`, `enum` o `string`: decide el widget de edición. */
export type SettingType = "bool" | "integer" | "real" | "enum" | "string";

export interface Setting {
  name: string;
  /** `null` en los parámetros que solo puede leer un superusuario (`data_directory`, `hba_file`, …). */
  value: string | null;
  unit: string | null;
  category: string;
  shortDesc: string;
  /** internal (nunca), postmaster (con reinicio), sighup (con recarga), o en caliente el resto. */
  context: string;
  varType: SettingType;
  minVal: string | null;
  maxVal: string | null;
  enumVals: string[];
  bootVal: string | null;
  resetVal: string | null;
  source: string;
  pendingRestart: boolean;
}

export type SettingChange =
  | { kind: "set"; name: string; value: string }
  | { kind: "reset"; name: string };

export const serverSettings = (id: string) => invoke<Setting[]>("server_settings", { id });

export const settingsPreview = (changes: SettingChange[]) =>
  invoke<DdlStatement[]>("settings_preview", { changes });

/** Devuelve `true` si algún cambio quedó pendiente de reinicio. */
export const settingsApply = (id: string, changes: SettingChange[]) =>
  invoke<boolean>("settings_apply", { id, changes });