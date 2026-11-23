/**
 * Lo que la pantalla de migraciones decide por su cuenta, sin servidor.
 *
 * La regla de verdad —qué se puede aplicar y en qué orden— es de `Report::check` en el núcleo, que
 * es lo que rechaza un comando. Esto es su sombra para la interfaz: apaga el botón que el núcleo
 * iba a rechazar y dice por qué, en vez de dejar que se apriete para recibir el error.
 */

import { t } from "./i18n.svelte";
import type { MigrationItem, MigrationState } from "./ipc";

export const STATE_LABEL: Record<MigrationState, string> = {
  get pending() { return t("Pendiente"); },
  get applied() { return t("Aplicada"); },
  get changed() { return t("Modificada"); },
  get failed() { return t("Falló"); },
};

/** Clases `tag-*` de `app.css`. */
export const STATE_TONE: Record<MigrationState, string> = {
  pending: "tag-neutral",
  applied: "tag-ok",
  changed: "tag-warn",
  failed: "tag-bad",
};

export const STATE_HINT: Record<MigrationState, string> = {
  get pending() { return t("Todavía no se aplicó a este servidor y esta base"); },
  get applied() { return t("Se aplicó y el archivo sigue igual"); },
  get changed() { return t("Se aplicó, pero el archivo cambió después: lo que hay en el servidor ya no es lo que dice la carpeta"); },
  get failed() { return t("El último intento terminó con error; se puede reintentar"); },
};

/** `V012` — el número tal como va en el nombre del archivo. */
export function versionLabel(version: number): string {
  return `V${String(version).padStart(3, "0")}`;
}

/** El tramo del nombre del archivo, legible: `crear_clientes` → `crear clientes`. */
export function descriptionText(description: string): string {
  return description.replace(/_/g, " ");
}

/**
 * Por qué no se puede aplicar `item` ahora, o `null` si se puede. Misma regla que `Report::check`:
 * ninguna modificada en el medio, y siempre la primera pendiente.
 */
export function applyBlocker(items: MigrationItem[], item: MigrationItem): string | null {
  if (item.state === "applied") return t("Ya está aplicada");
  const changed = items.find((other) => other.state === "changed");
  if (changed) {
    return t("{versionLabel} se aplicó con otro contenido: resolverlo antes de aplicar más", { versionLabel: versionLabel(changed.version) });
  }
  const before = items.find(
    (other) => other.version < item.version && (other.state === "pending" || other.state === "failed"),
  );
  if (before) return t("Antes falta aplicar {versionLabel}", { versionLabel: versionLabel(before.version) });
  return null;
}

/** Por qué no se puede deshacer `item`, o `null`. Solo la última aplicada, y con su `U…`. */
export function revertBlocker(items: MigrationItem[], item: MigrationItem): string | null {
  if (item.state !== "applied" && item.state !== "changed") return t("No está aplicada");
  if (!item.hasDown) return t("No tiene un script de reversa (U{versionLabel}__…)", { versionLabel: versionLabel(item.version).slice(1) });
  const later = items.find(
    (other) => other.version > item.version && (other.state === "applied" || other.state === "changed"),
  );
  if (later) return t("{versionLabel} se aplicó después: deshacer empezando por la última", { versionLabel: versionLabel(later.version) });
  return null;
}

/** La descripción que se propone al guardar: la del comparar, o vacía si no hay nada que decir. */
export function suggestDescription(schema: string): string {
  return schema ? t("sincronizar {schema}", { schema }) : "";
}

/** Huella corta para mostrar; la entera va en el `title`. */
export function shortSum(checksum: string): string {
  return checksum.slice(0, 10);
}
