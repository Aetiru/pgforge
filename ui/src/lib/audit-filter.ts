/**
 * El formulario de filtros de la auditoría, convertido en el `AuditFilter` que entiende el núcleo.
 *
 * Puro y con Vitest: lo delicado es el rango de fechas. Un `<input type="date">` entrega `2026-03-04`
 * sin hora, y «hasta el 4» tiene que incluir todo el día 4, no su primer segundo.
 */

import type { AuditFilter, AuditFormat } from "./ipc";

export interface AuditForm {
  profileId: string;
  database: string;
  /** `yyyy-mm-dd` o vacío. */
  from: string;
  to: string;
  source: "" | "editor" | "dialog";
  environment: "" | "dev" | "test" | "prod";
  onlyFailed: boolean;
  text: string;
}

export const EMPTY_FORM: AuditForm = {
  profileId: "",
  database: "",
  from: "",
  to: "",
  source: "",
  environment: "",
  onlyFailed: false,
  text: "",
};

/** Segundos desde el epoch de las 00:00 locales del día `yyyy-mm-dd`, o `null` si no es una fecha. */
export function dayStart(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]) - 1, Number(match[3])];
  const date = new Date(year, month, day, 0, 0, 0, 0);
  // `new Date(2026, 1, 31)` rueda a marzo: una fecha inexistente no es una fecha.
  if (date.getFullYear() !== year || date.getMonth() !== month || date.getDate() !== day) return null;
  return Math.floor(date.getTime() / 1000);
}

/** El último segundo de ese día local. */
export function dayEnd(value: string): number | null {
  const start = dayStart(value);
  if (start === null) return null;
  const [year, month, day] = value.split("-").map(Number);
  const next = new Date(year, month - 1, day + 1, 0, 0, 0, 0);
  return Math.floor(next.getTime() / 1000) - 1;
}

export function toFilter(form: AuditForm): AuditFilter {
  const text = form.text.trim();
  return {
    profileId: form.profileId || null,
    database: form.database || null,
    from: dayStart(form.from),
    to: dayEnd(form.to),
    source: form.source || null,
    environment: form.environment || null,
    onlyFailed: form.onlyFailed,
    text: text || null,
  };
}

/** Cuántos filtros hay puestos, para el rótulo de «Limpiar». */
export function activeFilters(form: AuditForm): number {
  const filter = toFilter(form);
  return [
    filter.profileId,
    filter.database,
    filter.from,
    filter.to,
    filter.source,
    filter.environment,
    filter.onlyFailed || null,
    filter.text,
  ].filter((value) => value !== null && value !== undefined).length;
}

/** Nombre propuesto para el archivo exportado: `auditoria-2026-03-04.csv`. */
export function exportName(format: AuditFormat, now: Date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  const day = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  return `auditoria-${day}.${format}`;
}
