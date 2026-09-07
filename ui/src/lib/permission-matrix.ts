import type { EffectivePrivilege } from "./ipc";
import type { MatrixObjectKind } from "./permission-matrix.svelte";

/**
 * Pivote puro de la matriz de permisos: `EffectivePrivilege[]` llega en formato largo
 * (objeto, rol, privilegio, otorgado, directo) y acá se arma lo que la grilla necesita para
 * dibujarse - la lista de objetos en su orden, y el acceso a una celda por las tres claves.
 */

/** El vocabulario de privilegios de cada familia, en el orden en que se muestran las columnas. */
export const PRIVILEGES_OF: Record<MatrixObjectKind, string[]> = {
  table: ["SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE", "REFERENCES", "TRIGGER"],
  sequence: ["USAGE", "SELECT", "UPDATE"],
  function: ["EXECUTE"],
};

/** Los objetos distintos, en el orden en que ya vienen del servidor (por nombre). */
export function objectsOf(effective: EffectivePrivilege[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const row of effective) {
    if (!seen.has(row.object)) {
      seen.add(row.object);
      out.push(row.object);
    }
  }
  return out;
}

const NUL = String.fromCharCode(0);

function cellKey(object: string, role: string, privilege: string): string {
  return object + NUL + role + NUL + privilege;
}

/** Indice para acceder a una celda por (objeto, rol, privilegio) en O(1). */
export function indexEffective(effective: EffectivePrivilege[]): Map<string, EffectivePrivilege> {
  const map = new Map<string, EffectivePrivilege>();
  for (const row of effective) {
    map.set(cellKey(row.object, row.role, row.privilege), row);
  }
  return map;
}

export function cellOf(
  index: Map<string, EffectivePrivilege>,
  object: string,
  role: string,
  privilege: string,
): EffectivePrivilege | null {
  return index.get(cellKey(object, role, privilege)) ?? null;
}
