/**
 * Lo que la pantalla de migraciones decide por su cuenta, sin servidor.
 *
 * La regla de verdad —qué se puede aplicar y en qué orden— es de `Report::check` en el núcleo, que
 * es lo que rechaza un comando. Esto es su sombra para la interfaz: apaga el botón que el núcleo
 * iba a rechazar y dice por qué, en vez de dejar que se apriete para recibir el error.
 */

import type { MigrationItem, MigrationState } from "./ipc";

export const STATE_LABEL: Record<MigrationState, string> = {
  pending: "Pendiente",
  applied: "Aplicada",
  changed: "Modificada",
  failed: "Falló",
};

/** Clases `tag-*` de `app.css`. */
export const STATE_TONE: Record<MigrationState, string> = {
  pending: "tag-neutral",
  applied: "tag-ok",
  changed: "tag-warn",
  failed: "tag-bad",
};

export const STATE_HINT: Record<MigrationState, string> = {
  pending: "Todavía no se aplicó a este servidor y esta base",
  applied: "Se aplicó y el archivo sigue igual",
  changed:
    "Se aplicó, pero el archivo cambió después: lo que hay en el servidor ya no es lo que dice la carpeta",
  failed: "El último intento terminó con error; se puede reintentar",
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
  if (item.state === "applied") return "Ya está aplicada";
  const changed = items.find((other) => other.state === "changed");
  if (changed) {
    return `${versionLabel(changed.version)} se aplicó con otro contenido: resolverlo antes de aplicar más`;
  }
  const before = items.find(
    (other) => other.version < item.version && (other.state === "pending" || other.state === "failed"),
  );
  if (before) return `Antes falta aplicar ${versionLabel(before.version)}`;
  return null;
}

/** Por qué no se puede deshacer `item`, o `null`. Solo la última aplicada, y con su `U…`. */
export function revertBlocker(items: MigrationItem[], item: MigrationItem): string | null {
  if (item.state !== "applied" && item.state !== "changed") return "No está aplicada";
  if (!item.hasDown) return `No tiene un script de reversa (U${versionLabel(item.version).slice(1)}__…)`;
  const later = items.find(
    (other) => other.version > item.version && (other.state === "applied" || other.state === "changed"),
  );
  if (later) return `${versionLabel(later.version)} se aplicó después: deshacer empezando por la última`;
  return null;
}

/** La descripción que se propone al guardar: la del comparar, o vacía si no hay nada que decir. */
export function suggestDescription(schema: string): string {
  return schema ? `sincronizar ${schema}` : "";
}

/** Huella corta para mostrar; la entera va en el `title`. */
export function shortSum(checksum: string): string {
  return checksum.slice(0, 10);
}
