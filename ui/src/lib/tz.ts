/**
 * Mostrar un `timestamptz` en otra zona horaria.
 *
 * PostgreSQL entrega la fecha ya convertida a la zona de la sesión (`2016-12-22 08:40:00-05`), así
 * que cambiar de zona es leer ese texto, volver al instante y escribirlo con otro desfase. Solo
 * cambia **lo que se ve**: el valor crudo —lo que se copia y lo que se edita— no se toca. Un texto
 * sin desfase (`timestamp`, `date`) no se reconoce y pasa igual, así que no hace falta saber el tipo
 * de la columna. Es puro y con Vitest.
 */


import { t } from "./i18n.svelte";
export type TimeZoneMode = "server" | "local" | "utc";

export const ZONE_MODES: { value: TimeZoneMode; label: string; hint: string }[] = [
  { value: "server", get label() { return t("Del servidor"); }, get hint() { return t("Como las entrega PostgreSQL, en la zona de la sesión"); } },
  { value: "local", get label() { return t("Esta computadora"); }, get hint() { return t("Convertidas a la zona de este equipo"); } },
  { value: "utc", label: "UTC", get hint() { return t("Convertidas a UTC"); } },
];

const STAMP =
  /^(\d{4})-(\d\d)-(\d\d) (\d\d):(\d\d):(\d\d)(\.\d+)?([+-])(\d\d)(?::?(\d\d))?(?::?(\d\d))?$/;

const pad = (n: number, width = 2) => String(n).padStart(width, "0");

/** `+05:30`, `-05` o `+00`: el mismo estilo corto que usa PostgreSQL. */
function offsetText(minutes: number): string {
  const sign = minutes < 0 ? "-" : "+";
  const abs = Math.abs(minutes);
  const hours = Math.floor(abs / 60);
  const rest = abs % 60;
  return rest === 0 ? `${sign}${pad(hours)}` : `${sign}${pad(hours)}:${pad(rest)}`;
}

/**
 * El texto en la zona pedida. `localOffset` es el desfase de este equipo en minutos para un
 * instante dado (se inyecta para poder probar sin depender de la zona de quien corre los tests).
 */
export function convertTimestamptz(
  text: string,
  mode: TimeZoneMode,
  localOffset: (utcMs: number) => number = (ms) => -new Date(ms).getTimezoneOffset(),
): string {
  if (mode === "server") return text;
  const match = STAMP.exec(text);
  if (!match) return text;

  const [, y, mo, d, h, mi, s, fraction = "", sign, oh, om = "0", os = "0"] = match;
  const offset = (Number(oh) * 60 + Number(om) + Number(os) / 60) * (sign === "-" ? -1 : 1);
  const utcMs = Date.UTC(Number(y), Number(mo) - 1, Number(d), Number(h), Number(mi), Number(s)) - offset * 60_000;
  if (Number.isNaN(utcMs)) return text;

  const target = mode === "utc" ? 0 : localOffset(utcMs);
  const shifted = new Date(utcMs + target * 60_000);
  return (
    `${pad(shifted.getUTCFullYear(), 4)}-${pad(shifted.getUTCMonth() + 1)}-${pad(shifted.getUTCDate())} ` +
    `${pad(shifted.getUTCHours())}:${pad(shifted.getUTCMinutes())}:${pad(shifted.getUTCSeconds())}` +
    `${fraction}${offsetText(target)}`
  );
}
