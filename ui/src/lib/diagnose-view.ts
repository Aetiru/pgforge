import { i18n, t } from "./i18n.svelte";
import type { DiagnoseLevel } from "./ipc";
import { bytes } from "./format";

/** Clase de pastilla (`.tag-*` de app.css) de cada nivel: el color lo decide un solo lugar. */
export function levelTag(level: DiagnoseLevel): string {
  switch (level) {
    case "attention":
      return "tag-bad";
    case "review":
      return "tag-warn";
    case "info":
      return "tag-info";
    default:
      return "tag-ok";
  }
}

export function levelLabel(level: DiagnoseLevel): string {
  switch (level) {
    case "attention":
      return t("Requiere atención");
    case "review":
      return t("Para revisar");
    case "info":
      return t("Informativo");
    default:
      return t("En orden");
  }
}

/** El peor de varios niveles; sin ninguno, «en orden». */
export function worstLevel(levels: DiagnoseLevel[]): DiagnoseLevel {
  const order: DiagnoseLevel[] = ["ok", "info", "review", "attention"];
  return levels.reduce<DiagnoseLevel>(
    (worst, level) => (order.indexOf(level) > order.indexOf(worst) ? level : worst),
    "ok",
  );
}

/**
 * Cuánto falta, dicho a la escala que importa. Un pronóstico no tiene precisión de días: «5 meses»
 * dice lo que hay que decidir, «147 días» pretende una exactitud que la recta no tiene.
 */
export function daysText(days: number | null | undefined): string {
  if (days === null || days === undefined || !Number.isFinite(days)) return "—";
  if (days < 1) return t("menos de un día");
  if (days < 60) return Math.round(days) === 1
      ? t("{days} día", { days: 1 })
      : t("{days} días", { days: Math.round(days) });
  if (days < 730) return t("{months} meses", { months: Math.round(days / 30) });
  const years = Math.round(days / 365);
  return years > 20 ? t("más de 20 años") : t("unos {years} años", { years });
}

/** «+2,1 GB/día»: lo que crece algo, con signo. */
export function growthText(bytesPerDay: number | null | undefined): string {
  if (bytesPerDay === null || bytesPerDay === undefined) return t("faltan muestras");
  const sign = bytesPerDay < 0 ? "−" : "+";
  return t("{amount}/día", { amount: `${sign}${bytes(Math.abs(bytesPerDay)).replace(".", ",")}` });
}

/** Edad de transacciones en millones: «1.480 M». */
export function millions(age: number): string {
  return `${Math.round(age / 1_000_000).toLocaleString(i18n.current)} M`;
}
