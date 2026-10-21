import {
  ALERT_KEYS,
  RULE_SPECS,
  defaultRules,
  evaluateAlerts,
  initialStates,
  normalizeRules,
  type AlertKey,
  type AlertLevel,
  type AlertRules,
  type AlertStates,
  type RuleConfig,
  type RuleStatus,
} from "./alerts";
import { explorer } from "./explorer.svelte";
import type { Metrics } from "./ipc";
import { notify } from "./notify.svelte";

/**
 * Las alertas del monitoreo: reglas por servidor, estado entre muestras e historial corto.
 *
 * **Solo corren mientras el monitoreo del servidor está abierto.** El sondeo vive en `monitor` y
 * lo arranca el montaje del dashboard; cerrarlo lo detiene, y con él las alertas. No hay vigilancia
 * en segundo plano y la interfaz no debe prometerla. La decisión de qué salta es de `evaluateAlerts`
 * (puro); acá solo se guarda, se anota y se avisa.
 *
 * Las reglas son una preferencia de la aplicación —no del perfil, así que no tocan
 * `connections.json`— y van a `localStorage` directo, igual que `notify.svelte.ts`: valen para todas
 * las ventanas. El historial también, porque «qué saltó mientras miraba otra cosa» es justo la
 * pregunta que se hace al volver.
 */

const RULES_KEY = "pgforge.alerts.rules";
const HISTORY_KEY = "pgforge.alerts.history";
const HISTORY_LIMIT = 30;

export interface AlertRecord {
  id: string;
  profileId: string;
  server: string;
  key: AlertKey;
  kind: "fired" | "repeat" | "resolved";
  level: AlertLevel;
  title: string;
  detail: string;
  at: number;
}

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    // Valor corrupto o sin `localStorage` (los tests corren en Node): se parte de cero.
    return fallback;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Sin `localStorage` no se recuerda esta vez.
  }
}

function loadRules(): Record<string, AlertRules> {
  const raw = read<Record<string, unknown>>(RULES_KEY, {});
  const rules: Record<string, AlertRules> = {};
  if (typeof raw === "object" && raw !== null) {
    for (const [id, value] of Object.entries(raw)) rules[id] = normalizeRules(value);
  }
  return rules;
}

function loadHistory(): AlertRecord[] {
  const raw = read<unknown>(HISTORY_KEY, []);
  return Array.isArray(raw)
    ? (raw as AlertRecord[])
        .filter((r) => r && typeof r.id === "string" && ALERT_KEYS.includes(r.key))
        .slice(0, HISTORY_LIMIT)
    : [];
}

class Alerts {
  private rulesById = $state<Record<string, AlertRules>>(loadRules());
  /** Lo más nuevo primero. */
  history = $state<AlertRecord[]>(loadHistory());
  /** Cómo está cada regla en la última muestra del servidor que se mira. */
  status = $state<Record<AlertKey, RuleStatus> | null>(null);

  private states = new Map<string, AlertStates>();

  rulesFor(profileId: string): AlertRules {
    return this.rulesById[profileId] ?? defaultRules();
  }

  setRule(profileId: string, key: AlertKey, patch: Partial<RuleConfig>) {
    const current = this.rulesFor(profileId);
    const spec = RULE_SPECS[key];
    const next: RuleConfig = { ...current[key], ...patch };
    // Un umbral que no es número o sale del rango se ignora: el campo numérico del panel emite
    // vacío mientras uno escribe, y eso no puede disparar una alerta a cada muestra.
    if (
      !Number.isFinite(next.threshold) ||
      next.threshold < spec.min ||
      next.threshold > spec.max
    ) {
      next.threshold = current[key].threshold;
    }
    this.rulesById = { ...this.rulesById, [profileId]: { ...current, [key]: next } };
    write(RULES_KEY, this.rulesById);
  }

  resetRules(profileId: string) {
    const rest = { ...this.rulesById };
    delete rest[profileId];
    this.rulesById = rest;
    write(RULES_KEY, this.rulesById);
  }

  /** Una muestra del monitoreo de `profileId`. */
  observe(profileId: string, metrics: Metrics, now = Date.now()) {
    const previous = this.states.get(profileId) ?? initialStates();
    const result = evaluateAlerts(metrics, this.rulesFor(profileId), previous, now);
    this.states.set(profileId, result.states);
    this.status = result.status;

    if (result.fired.length === 0 && result.resolved.length === 0) return;

    const server = explorer.profiles.find((p) => p.id === profileId)?.name ?? profileId;
    const records: AlertRecord[] = [];

    for (const alert of result.fired) {
      records.push({
        id: crypto.randomUUID(),
        profileId,
        server,
        key: alert.key,
        kind: alert.repeat ? "repeat" : "fired",
        level: alert.level,
        title: alert.title,
        detail: alert.detail,
        at: alert.at,
      });
      // El aviso del sistema es un extra: que falle (permiso negado, sin servicio de
      // notificaciones) no toca el historial ni el resto del monitoreo, y `notify` ya lo traga.
      void notify
        .alert(
          `${profileId}:${alert.key}:${Math.floor(alert.at / 60_000)}`,
          `${server}: ${alert.title}`,
          alert.detail,
        )
        .catch(() => {});
    }
    for (const resolved of result.resolved) {
      records.push({
        id: crypto.randomUUID(),
        profileId,
        server,
        key: resolved.key,
        kind: "resolved",
        level: "warn",
        title: resolved.title,
        detail: "Se resolvió",
        at: resolved.at,
      });
    }

    this.history = [...records.reverse(), ...this.history].slice(0, HISTORY_LIMIT);
    write(HISTORY_KEY, this.history);
  }

  /** Al cerrar el monitoreo: la cuenta de «desde cuándo» no puede seguir corriendo sin muestras. */
  reset(profileId: string) {
    this.states.delete(profileId);
    this.status = null;
  }

  clearHistory() {
    this.history = [];
    write(HISTORY_KEY, this.history);
  }
}

export const alerts = new Alerts();
