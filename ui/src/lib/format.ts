/** Formatos compartidos por las tablas del dashboard. */

const UNITS = ["B", "kB", "MB", "GB", "TB", "PB"];

export function bytes(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  if (value === 0) return "0 B";

  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size < 10 && unit > 0 ? size.toFixed(1) : Math.round(size)} ${UNITS[unit]}`;
}

/**
 * Duraciones legibles de un vistazo.
 *
 * En un dashboard de sesiones lo que importa es el orden de magnitud —¿esta consulta lleva
 * segundos o media hora?—, no el detalle al milisegundo.
 */
export function duration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return "—";
  if (seconds < 1) return `${Math.round(seconds * 1000)} ms`;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ${Math.round(seconds % 60)} s`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} h ${Math.round((seconds % 3600) / 60)} min`;
  return `${Math.floor(seconds / 86400)} d`;
}

/** Igual que `duration`, pero para un instante del pasado. */
export function ago(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return "nunca";
  return `hace ${duration(seconds)}`;
}

export function count(value: number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  return value.toLocaleString("es");
}

export function percent(ratio: number | null | undefined, decimals = 1): string {
  if (ratio === null || ratio === undefined) return "—";
  return `${(ratio * 100).toFixed(decimals)} %`;
}

export function decimal(value: number | null | undefined, decimals = 1): string {
  if (value === null || value === undefined) return "—";
  return value.toFixed(decimals);
}

/** Deja la consulta en una línea para que entre en una celda. */
export function oneLine(text: string | null | undefined, max = 300): string {
  if (!text) return "";
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max)}…` : flat;
}
