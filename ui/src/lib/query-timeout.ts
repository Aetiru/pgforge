/**
 * Tiempo máximo por sentencia de una pestaña de consulta.
 *
 * `null` es «el del perfil»: no se toca la sesión, o se la devuelve al valor con que se abrió. `0`
 * es «sin límite», que es lo que `statement_timeout = 0` significa en PostgreSQL. En producción una
 * pestaña nueva arranca con cinco minutos si el perfil no trae un límite propio: una consulta
 * olvidada corriendo horas ahí es el caso que más cuesta.
 */


import { t } from "./i18n.svelte";
export const TIMEOUT_OPTIONS: { label: string; ms: number | null }[] = [
  { get label() { return t("Tiempo: el del perfil"); }, ms: null },
  { label: "30 s", ms: 30_000 },
  { label: "1 min", ms: 60_000 },
  { label: "5 min", ms: 300_000 },
  { label: "30 min", ms: 1_800_000 },
  { get label() { return t("Sin límite"); }, ms: 0 },
];

/** Lo que se pone al abrir una pestaña, o `null` si no se toca nada. */
export function defaultTimeout(environment: string | null | undefined, profileTimeoutMs: number | undefined): number | null {
  if (environment !== "prod") return null;
  // Un límite propio del perfil manda: pisarlo con cinco minutos podría aflojar uno más estricto.
  if (profileTimeoutMs !== undefined && profileTimeoutMs > 0) return null;
  return 300_000;
}
