import { connectionLevel } from "./health";
import { duration } from "./format";
import type { Metrics } from "./ipc";

/**
 * Alertas del monitoreo: reglas, y la función pura que decide cuáles saltan.
 *
 * Son cinco a propósito. Una lista de veinte avisos tibios se aprende a ignorar entera, así que
 * cada regla señala algo que pide una decisión y su umbral por omisión es alto. Lo que no puede
 * pasar es que esta pantalla y el veredicto de `health.ts` discrepen sobre qué es «rojo»: la de
 * conexiones se apoya en `connectionLevel`, y con el umbral por omisión dice lo mismo que la ficha.
 *
 * `evaluateAlerts` no toca el reloj, el disco ni el sistema: recibe las métricas, las reglas, el
 * estado previo y la hora, y devuelve el estado nuevo con lo que hay que avisar. El store
 * (`alerts.svelte.ts`) es quien notifica; acá se prueba todo lo que puede salir mal en una alerta
 * —el spam, el parpadeo, la falta de permisos— sin servidor ni ventana.
 */

export const ALERT_KEYS = ["blocked", "idleTx", "connections", "replication", "cache"] as const;
export type AlertKey = (typeof ALERT_KEYS)[number];

export type AlertLevel = "warn" | "bad";

export interface RuleConfig {
  enabled: boolean;
  /** En la unidad de la regla (ver `RULE_SPECS`). */
  threshold: number;
}

export type AlertRules = Record<AlertKey, RuleConfig>;

interface RuleSpec {
  label: string;
  /** Lo que se escribe después del número en el panel. */
  unit: string;
  min: number;
  max: number;
  step: number;
  threshold: number;
  enabled: boolean;
  level: AlertLevel;
  direction: "above" | "below";
  /**
   * Cuánto tiene que sostenerse el incumplimiento antes de avisar. Cero cuando la regla ya mide una
   * duración (una sesión bloqueada hace 30 s ya lleva 30 s así); distinto de cero cuando el valor
   * es instantáneo y un pico de un ciclo no dice nada.
   */
  holdSeconds: number;
  /** Desde dónde una alerta encendida se da por resuelta: la histéresis contra el parpadeo. */
  clearAt: (threshold: number) => number;
  description: string;
}

export const RULE_SPECS: Record<AlertKey, RuleSpec> = {
  blocked: {
    label: "Sesión bloqueada",
    unit: "segundos esperando",
    min: 5,
    max: 3600,
    step: 5,
    threshold: 30,
    enabled: true,
    level: "bad",
    direction: "above",
    holdSeconds: 0,
    clearAt: (t) => t * 0.8,
    description: "Una sesión espera a otra más de este tiempo.",
  },
  idleTx: {
    label: "Transacción abierta sin trabajar",
    unit: "minutos",
    min: 1,
    max: 1440,
    step: 1,
    threshold: 10,
    enabled: true,
    level: "warn",
    direction: "above",
    holdSeconds: 0,
    clearAt: (t) => t * 0.8,
    description: "Una sesión lleva más de este tiempo en «idle in transaction»: retiene candados.",
  },
  connections: {
    label: "Conexiones cerca del máximo",
    unit: "% de max_connections",
    min: 50,
    max: 99,
    step: 1,
    threshold: 90,
    enabled: true,
    level: "bad",
    direction: "above",
    holdSeconds: 30,
    clearAt: (t) => t - 5,
    description: "La tabla de conexiones supera este porcentaje, sostenido medio minuto.",
  },
  replication: {
    label: "Retraso de réplica",
    unit: "segundos de retraso",
    min: 5,
    max: 86400,
    step: 5,
    threshold: 60,
    enabled: false,
    level: "bad",
    direction: "above",
    holdSeconds: 0,
    clearAt: (t) => t * 0.8,
    description: "La réplica está más atrás que esto. Apagada por omisión: solo importa con réplicas.",
  },
  cache: {
    label: "Caché de lectura bajo",
    unit: "% de aciertos",
    min: 10,
    max: 99,
    step: 1,
    threshold: 80,
    enabled: false,
    level: "warn",
    direction: "below",
    holdSeconds: 300,
    clearAt: (t) => t + 3,
    description: "Los aciertos de caché se mantienen por debajo de esto durante cinco minutos.",
  },
};

export function defaultRules(): AlertRules {
  const rules = {} as AlertRules;
  for (const key of ALERT_KEYS) {
    rules[key] = { enabled: RULE_SPECS[key].enabled, threshold: RULE_SPECS[key].threshold };
  }
  return rules;
}

/**
 * Lo guardado puede venir viejo, a medias o corrupto: se completa con los valores por omisión y un
 * umbral fuera de rango o que no es número vuelve al de fábrica en vez de disparar a cada muestra.
 */
export function normalizeRules(raw: unknown): AlertRules {
  const rules = defaultRules();
  if (typeof raw !== "object" || raw === null) return rules;
  for (const key of ALERT_KEYS) {
    const stored = (raw as Record<string, unknown>)[key];
    if (typeof stored !== "object" || stored === null) continue;
    const { enabled, threshold } = stored as Record<string, unknown>;
    const spec = RULE_SPECS[key];
    if (typeof enabled === "boolean") rules[key].enabled = enabled;
    if (
      typeof threshold === "number" &&
      Number.isFinite(threshold) &&
      threshold >= spec.min &&
      threshold <= spec.max
    ) {
      rules[key].threshold = threshold;
    }
  }
  return rules;
}

/** Cuánto después se repite el aviso de una alerta que sigue encendida. */
export const REMIND_MS = 30 * 60 * 1000;

export interface AlertState {
  firing: boolean;
  /** Desde cuándo se incumple la regla sin interrupción; `null` si ahora no. */
  breachSince: number | null;
  lastNotifiedAt: number | null;
}

export type AlertStates = Record<AlertKey, AlertState>;

export function initialStates(): AlertStates {
  const states = {} as AlertStates;
  for (const key of ALERT_KEYS) states[key] = idle();
  return states;
}

function idle(): AlertState {
  return { firing: false, breachSince: null, lastNotifiedAt: null };
}

export interface Alert {
  key: AlertKey;
  level: AlertLevel;
  title: string;
  detail: string;
  at: number;
  /** Recordatorio de una alerta que sigue encendida, no una nueva. */
  repeat: boolean;
}

export interface Resolved {
  key: AlertKey;
  title: string;
  at: number;
}

/** Cómo está la regla ahora, para mostrarla en el panel. */
export type RuleStatus =
  | { kind: "off" }
  | { kind: "ok"; value: number | null }
  | { kind: "pending"; value: number }
  | { kind: "firing"; value: number }
  /** No se puede medir: el rol no ve lo que la regla necesita. */
  | { kind: "restricted"; reason: string }
  /** Hoy no hay dato (primera muestra, sin réplicas que informen). */
  | { kind: "unknown" };

export interface Evaluation {
  states: AlertStates;
  fired: Alert[];
  resolved: Resolved[];
  status: Record<AlertKey, RuleStatus>;
}

type Measure =
  | { kind: "value"; value: number; breached: boolean }
  /** La condición no existe (nadie bloqueado, sin réplicas): cuenta como cumplida. */
  | { kind: "absent" }
  | { kind: "unknown" }
  | { kind: "restricted"; reason: string };

function measure(key: AlertKey, metrics: Metrics, threshold: number): Measure {
  switch (key) {
    case "blocked": {
      const seconds = metrics.longestBlockedSeconds;
      return seconds === null
        ? { kind: "absent" }
        : { kind: "value", value: seconds, breached: seconds > threshold };
    }
    case "idleTx": {
      const seconds = metrics.longestIdleInTransactionSeconds;
      if (seconds === null) return { kind: "absent" };
      const minutes = seconds / 60;
      return { kind: "value", value: minutes, breached: minutes > threshold };
    }
    case "connections": {
      if (metrics.maxConnections <= 0) return { kind: "absent" };
      const percent = (metrics.totalConnections / metrics.maxConnections) * 100;
      // La definición de «rojo» es la de `health.ts`; acá solo se le pasa el umbral elegido.
      const bad = connectionLevel(metrics.totalConnections, metrics.maxConnections, threshold / 100);
      return { kind: "value", value: percent, breached: bad === "bad" };
    }
    case "replication": {
      const lag = metrics.replication;
      if (lag.kind === "none") return { kind: "absent" };
      if (lag.kind === "restricted") {
        return {
          kind: "restricted",
          reason: "sin permisos: el rol no puede ver el progreso de las réplicas (pg_read_all_stats)",
        };
      }
      return lag.seconds === null
        ? { kind: "unknown" }
        : { kind: "value", value: lag.seconds, breached: lag.seconds > threshold };
    }
    case "cache": {
      const ratio = metrics.cacheHitRatio;
      if (ratio === null) return { kind: "unknown" };
      const percent = ratio * 100;
      return { kind: "value", value: percent, breached: percent < threshold };
    }
  }
}

function describe(key: AlertKey, value: number, metrics: Metrics): { title: string; detail: string } {
  switch (key) {
    case "blocked":
      return {
        title: "Sesión bloqueada",
        detail: `Una sesión lleva ${duration(value)} esperando a otra (${metrics.waitingConnections} en espera)`,
      };
    case "idleTx":
      return {
        title: "Transacción abierta sin trabajar",
        detail: `Una sesión lleva ${duration(value * 60)} en «idle in transaction»`,
      };
    case "connections":
      return {
        title: "Conexiones cerca del máximo",
        detail: `${Math.round(value)} % del máximo (${metrics.totalConnections} de ${metrics.maxConnections})`,
      };
    case "replication":
      return { title: "Réplica atrasada", detail: `Retraso de ${duration(value)}` };
    case "cache":
      return { title: "Caché de lectura bajo", detail: `Aciertos de caché en ${value.toFixed(1)} %` };
  }
}

/**
 * Qué alertas saltan, cuáles se resuelven y cómo queda el estado.
 *
 * - Una regla incumplida salta después de `holdSeconds` ininterrumpidos y **una sola vez**: mientras
 *   siga encendida solo se repite como recordatorio cada `REMIND_MS`.
 * - Se resuelve cuando el valor cruza `clearAt`, no el umbral: entre los dos queda encendida. Sin
 *   esa banda, un valor que oscila alrededor del límite avisaría a cada vuelta.
 * - Si no se puede medir (rol sin permisos, sin dato) el estado no se toca: no saltar es correcto,
 *   pero resolver una alerta por no ver sería avisar «ya pasó» sin saberlo.
 * - Una regla apagada pierde su estado sin avisar nada: apagarla es la respuesta, no un evento.
 */
export function evaluateAlerts(
  metrics: Metrics,
  rules: AlertRules,
  previous: AlertStates,
  now: number,
): Evaluation {
  const states = {} as AlertStates;
  const status = {} as Record<AlertKey, RuleStatus>;
  const fired: Alert[] = [];
  const resolved: Resolved[] = [];

  for (const key of ALERT_KEYS) {
    const rule = rules[key];
    const spec = RULE_SPECS[key];
    const before = previous[key] ?? idle();

    if (!rule.enabled) {
      states[key] = idle();
      status[key] = { kind: "off" };
      continue;
    }

    const m = measure(key, metrics, rule.threshold);

    if (m.kind === "unknown") {
      states[key] = before;
      status[key] = { kind: "unknown" };
      continue;
    }
    if (m.kind === "restricted") {
      states[key] = before;
      status[key] = { kind: "restricted", reason: m.reason };
      continue;
    }

    const cleared =
      m.kind === "absent" ||
      (spec.direction === "above"
        ? m.value <= spec.clearAt(rule.threshold)
        : m.value >= spec.clearAt(rule.threshold));

    if (m.kind === "absent" || (cleared && !m.breached)) {
      if (before.firing) resolved.push({ key, title: spec.label, at: now });
      states[key] = idle();
      status[key] = { kind: "ok", value: m.kind === "value" ? m.value : null };
      continue;
    }

    if (!m.breached) {
      // Banda de histéresis: ya no incumple pero todavía no se puede dar por resuelta.
      if (before.firing) {
        states[key] = remind(key, before, now, fired, metrics, m.value, spec.level);
        status[key] = { kind: "firing", value: m.value };
      } else {
        states[key] = idle();
        status[key] = { kind: "ok", value: m.value };
      }
      continue;
    }

    const breachSince = before.breachSince ?? now;
    if (before.firing) {
      states[key] = remind(key, { ...before, breachSince }, now, fired, metrics, m.value, spec.level);
      status[key] = { kind: "firing", value: m.value };
    } else if (now - breachSince >= spec.holdSeconds * 1000) {
      const text = describe(key, m.value, metrics);
      fired.push({ key, level: spec.level, ...text, at: now, repeat: false });
      states[key] = { firing: true, breachSince, lastNotifiedAt: now };
      status[key] = { kind: "firing", value: m.value };
    } else {
      states[key] = { firing: false, breachSince, lastNotifiedAt: null };
      status[key] = { kind: "pending", value: m.value };
    }
  }

  return { states, fired, resolved, status };
}

function remind(
  key: AlertKey,
  state: AlertState,
  now: number,
  fired: Alert[],
  metrics: Metrics,
  value: number,
  level: AlertLevel,
): AlertState {
  const last = state.lastNotifiedAt ?? 0;
  if (now - last < REMIND_MS) return state;
  fired.push({ key, level, ...describe(key, value, metrics), at: now, repeat: true });
  return { ...state, lastNotifiedAt: now };
}
