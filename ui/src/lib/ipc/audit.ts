/**
 * Registro de auditoría: el historial con filtros y exportable.
 *
 * Espejo de `pgforge_core::sql::audit`. La exportación pasa la ruta que devolvió `pickSave`: el
 * comando la verifica contra los diálogos nativos, igual que el resto de los que escriben archivos.
 */

import { invoke } from "./core";
import type { HistoryEntry } from "./query";

/** Todo lo que viene vacío no filtra. Fechas en segundos desde el epoch, ambos extremos incluidos. */
export interface AuditFilter {
  profileId?: string | null;
  database?: string | null;
  from?: number | null;
  to?: number | null;
  source?: "editor" | "dialog" | null;
  environment?: "dev" | "test" | "prod" | null;
  onlyFailed?: boolean;
  text?: string | null;
}

export interface AuditFacets {
  servers: { profileId: string; serverName: string | null }[];
  databases: string[];
}

export interface AuditPage {
  /** Cuántas filas cumplen el filtro, no solo las de esta página. */
  total: number;
  entries: HistoryEntry[];
}

export type AuditFormat = "csv" | "json";

export const auditQuery = (filter: AuditFilter, limit: number, offset: number) =>
  invoke<AuditPage>("audit_query", { filter, limit, offset });

export const auditFacets = () => invoke<AuditFacets>("audit_facets");

/** Escribe todo lo que cumple el filtro y devuelve cuántas filas fueron. */
export const auditExport = (filter: AuditFilter, format: AuditFormat, path: string) =>
  invoke<number>("audit_export", { filter, format, path });
