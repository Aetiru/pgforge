import { describe, expect, it } from "vitest";

import {
  ALERT_KEYS,
  REMIND_MS,
  defaultRules,
  evaluateAlerts,
  initialStates,
  normalizeRules,
  type AlertRules,
  type AlertStates,
} from "./alerts";
import { assessHealth } from "./health";
import type { Metrics } from "./ipc";

const calm: Metrics = {
  totalConnections: 10,
  activeConnections: 2,
  idleInTransaction: 0,
  waitingConnections: 0,
  maxConnections: 100,
  transactionsPerSecond: 12,
  cacheHitRatio: 0.99,
  longestTransactionSeconds: 1,
  longestBlockedSeconds: null,
  longestIdleInTransactionSeconds: null,
  replication: { kind: "none" },
};

const T0 = 1_000_000;
const SEC = 1000;
const MIN = 60 * SEC;

function allOn(): AlertRules {
  const rules = defaultRules();
  for (const key of ALERT_KEYS) rules[key].enabled = true;
  return rules;
}

/** Corre una secuencia de muestras (con su hora) y junta lo que fue saltando. */
function run(samples: [Partial<Metrics>, number][], rules = allOn()) {
  let states: AlertStates = initialStates();
  const fired: string[] = [];
  const resolved: string[] = [];
  let last = evaluateAlerts(calm, rules, states, T0);
  for (const [partial, at] of samples) {
    last = evaluateAlerts({ ...calm, ...partial }, rules, states, at);
    states = last.states;
    fired.push(...last.fired.map((a) => `${a.key}${a.repeat ? "*" : ""}`));
    resolved.push(...last.resolved.map((r) => r.key));
  }
  return { fired, resolved, last, states };
}

describe("reglas guardadas", () => {
  it("por omisión hay pocas encendidas y con umbrales altos", () => {
    const rules = defaultRules();
    expect(rules.blocked).toEqual({ enabled: true, threshold: 30 });
    expect(rules.idleTx).toEqual({ enabled: true, threshold: 10 });
    expect(rules.connections).toEqual({ enabled: true, threshold: 90 });
    // Las que solo importan con réplicas o con una carga conocida arrancan apagadas.
    expect(rules.replication.enabled).toBe(false);
    expect(rules.cache.enabled).toBe(false);
  });

  it("completa lo que falta y descarta lo corrupto", () => {
    const rules = normalizeRules({
      blocked: { enabled: false, threshold: 120 },
      idleTx: { enabled: "sí", threshold: -3 },
      connections: { threshold: Number.NaN },
      cache: 7,
    });
    expect(rules.blocked).toEqual({ enabled: false, threshold: 120 });
    expect(rules.idleTx).toEqual(defaultRules().idleTx);
    expect(rules.connections).toEqual(defaultRules().connections);
    expect(rules.cache).toEqual(defaultRules().cache);
    expect(normalizeRules(null)).toEqual(defaultRules());
    expect(normalizeRules("basura")).toEqual(defaultRules());
  });

  it("un umbral fuera de rango vuelve al de fábrica", () => {
    expect(normalizeRules({ connections: { enabled: true, threshold: 150 } }).connections.threshold).toBe(90);
    expect(normalizeRules({ connections: { enabled: true, threshold: 10 } }).connections.threshold).toBe(90);
  });
});

describe("sesión bloqueada", () => {
  it("salta al pasar el umbral y no antes", () => {
    const { fired } = run([
      [{ longestBlockedSeconds: 29 }, T0],
      [{ longestBlockedSeconds: 30 }, T0 + 2 * SEC],
      [{ longestBlockedSeconds: 31 }, T0 + 4 * SEC],
    ]);
    expect(fired).toEqual(["blocked"]);
  });

  it("avisa una sola vez mientras siga encendida", () => {
    const samples: [Partial<Metrics>, number][] = [];
    for (let i = 0; i < 50; i++) samples.push([{ longestBlockedSeconds: 60 + i }, T0 + i * 2 * SEC]);
    expect(run(samples).fired).toEqual(["blocked"]);
  });

  it("se resuelve cuando nadie queda esperando", () => {
    const { fired, resolved, states } = run([
      [{ longestBlockedSeconds: 45 }, T0],
      [{ longestBlockedSeconds: null }, T0 + 2 * SEC],
    ]);
    expect(fired).toEqual(["blocked"]);
    expect(resolved).toEqual(["blocked"]);
    expect(states.blocked.firing).toBe(false);
  });

  it("la histéresis evita avisar de nuevo cuando oscila alrededor del umbral", () => {
    const { fired, resolved } = run([
      [{ longestBlockedSeconds: 35 }, T0],
      [{ longestBlockedSeconds: 28 }, T0 + 2 * SEC],
      [{ longestBlockedSeconds: 33 }, T0 + 4 * SEC],
      [{ longestBlockedSeconds: 29 }, T0 + 6 * SEC],
      [{ longestBlockedSeconds: 31 }, T0 + 8 * SEC],
    ]);
    expect(fired).toEqual(["blocked"]);
    expect(resolved).toEqual([]);
  });

  it("al bajar de la banda se resuelve, y si vuelve es un aviso nuevo", () => {
    const { fired, resolved } = run([
      [{ longestBlockedSeconds: 40 }, T0],
      [{ longestBlockedSeconds: 10 }, T0 + 2 * SEC],
      [{ longestBlockedSeconds: 50 }, T0 + 4 * SEC],
    ]);
    expect(fired).toEqual(["blocked", "blocked"]);
    expect(resolved).toEqual(["blocked"]);
  });

  it("repite como recordatorio solo pasado el intervalo", () => {
    const { fired } = run([
      [{ longestBlockedSeconds: 40 }, T0],
      [{ longestBlockedSeconds: 50 }, T0 + REMIND_MS - 1],
      [{ longestBlockedSeconds: 50 }, T0 + REMIND_MS],
      [{ longestBlockedSeconds: 50 }, T0 + REMIND_MS + 2 * SEC],
    ]);
    expect(fired).toEqual(["blocked", "blocked*"]);
  });
});

describe("transacción abierta sin trabajar", () => {
  it("compara en minutos", () => {
    const { fired, last } = run([
      [{ longestIdleInTransactionSeconds: 9 * 60 }, T0],
      [{ longestIdleInTransactionSeconds: 11 * 60 }, T0 + 2 * SEC],
    ]);
    expect(fired).toEqual(["idleTx"]);
    expect(last.fired[0].detail).toContain("11 min");
    expect(last.fired[0].level).toBe("warn");
  });

  it("se resuelve cuando la sesión termina su transacción", () => {
    const { resolved } = run([
      [{ longestIdleInTransactionSeconds: 12 * 60 }, T0],
      [{ longestIdleInTransactionSeconds: null }, T0 + 2 * SEC],
    ]);
    expect(resolved).toEqual(["idleTx"]);
  });
});

describe("conexiones", () => {
  it("espera a que se sostenga antes de avisar: un pico de un ciclo no es una alerta", () => {
    const { fired } = run([
      [{ totalConnections: 95 }, T0],
      [{ totalConnections: 50 }, T0 + 2 * SEC],
    ]);
    expect(fired).toEqual([]);
  });

  it("salta tras sostenerse y dice el porcentaje", () => {
    const { fired, last } = run([
      [{ totalConnections: 95 }, T0],
      [{ totalConnections: 96 }, T0 + 20 * SEC],
      [{ totalConnections: 96 }, T0 + 30 * SEC],
    ]);
    expect(fired).toEqual(["connections"]);
    expect(last.fired[0].detail).toContain("96 %");
    expect(last.fired[0].level).toBe("bad");
  });

  it("un pico interrumpido reinicia la cuenta", () => {
    const { fired } = run([
      [{ totalConnections: 95 }, T0],
      [{ totalConnections: 60 }, T0 + 20 * SEC],
      [{ totalConnections: 95 }, T0 + 25 * SEC],
      [{ totalConnections: 95 }, T0 + 50 * SEC],
    ]);
    expect(fired).toEqual([]);
  });

  it("con el umbral por omisión coincide con el «rojo» del veredicto de salud", () => {
    // Una sola definición de rojo: lo que la alerta considera incumplido es lo que `health.ts`
    // marca como «bad» por conexiones, en el borde y a cada lado.
    for (const total of [70, 89, 90, 91, 99]) {
      const metrics = { ...calm, totalConnections: total };
      const red = assessHealth(metrics).issues.some(
        (issue) => issue.level === "bad" && issue.text.startsWith("Conexiones"),
      );
      const states = initialStates();
      // El hold de 30 s ya cumplido: una sola muestra con `breachSince` previo.
      states.connections = { firing: false, breachSince: T0 - 31 * SEC, lastNotifiedAt: null };
      const { fired } = evaluateAlerts(metrics, allOn(), states, T0);
      expect(fired.some((a) => a.key === "connections"), `total=${total}`).toBe(red);
    }
  });

  it("respeta el umbral elegido", () => {
    const rules = allOn();
    rules.connections.threshold = 70;
    const states = initialStates();
    states.connections = { firing: false, breachSince: T0 - 60 * SEC, lastNotifiedAt: null };
    const { fired } = evaluateAlerts({ ...calm, totalConnections: 75 }, rules, states, T0);
    expect(fired.map((a) => a.key)).toEqual(["connections"]);
  });

  it("sin max_connections no hay nada que medir", () => {
    const { fired } = run([[{ maxConnections: 0, totalConnections: 5 }, T0]]);
    expect(fired).toEqual([]);
  });
});

describe("retraso de réplica", () => {
  const rules = () => {
    const r = allOn();
    r.replication.threshold = 60;
    return r;
  };

  it("salta con el retraso de una primaria y se resuelve al alcanzarla", () => {
    const { fired, resolved } = run(
      [
        [{ replication: { kind: "primary", replicas: 1, seconds: 90, bytes: 1 } }, T0],
        [{ replication: { kind: "primary", replicas: 1, seconds: 2, bytes: 1 } }, T0 + 2 * SEC],
      ],
      rules(),
    );
    expect(fired).toEqual(["replication"]);
    expect(resolved).toEqual(["replication"]);
  });

  it("también mide desde la réplica", () => {
    const { fired } = run(
      [[{ replication: { kind: "standby", seconds: 120, bytes: null } }, T0]],
      rules(),
    );
    expect(fired).toEqual(["replication"]);
  });

  it("sin permisos no falla ni salta: lo informa", () => {
    const { fired, last } = run([[{ replication: { kind: "restricted" } }, T0]], rules());
    expect(fired).toEqual([]);
    expect(last.status.replication.kind).toBe("restricted");
    if (last.status.replication.kind === "restricted") {
      expect(last.status.replication.reason).toContain("sin permisos");
    }
  });

  it("perder la visibilidad no resuelve una alerta encendida", () => {
    const { resolved, states } = run(
      [
        [{ replication: { kind: "primary", replicas: 1, seconds: 90, bytes: 1 } }, T0],
        [{ replication: { kind: "restricted" } }, T0 + 2 * SEC],
      ],
      rules(),
    );
    expect(resolved).toEqual([]);
    expect(states.replication.firing).toBe(true);
  });

  it("un retraso sin medir (réplica recién conectada) no cuenta", () => {
    const { fired, last } = run(
      [[{ replication: { kind: "primary", replicas: 1, seconds: null, bytes: 0 } }, T0]],
      rules(),
    );
    expect(fired).toEqual([]);
    expect(last.status.replication.kind).toBe("unknown");
  });

  it("quedarse sin réplicas resuelve la alerta", () => {
    const { resolved } = run(
      [
        [{ replication: { kind: "primary", replicas: 1, seconds: 90, bytes: 1 } }, T0],
        [{ replication: { kind: "none" } }, T0 + 2 * SEC],
      ],
      rules(),
    );
    expect(resolved).toEqual(["replication"]);
  });
});

describe("caché bajo sostenido", () => {
  it("un valle corto no avisa", () => {
    const { fired } = run([
      [{ cacheHitRatio: 0.5 }, T0],
      [{ cacheHitRatio: 0.5 }, T0 + 2 * MIN],
      [{ cacheHitRatio: 0.99 }, T0 + 3 * MIN],
      [{ cacheHitRatio: 0.5 }, T0 + 4 * MIN],
    ]);
    expect(fired).toEqual([]);
  });

  it("salta tras cinco minutos seguidos por debajo", () => {
    const { fired, last } = run([
      [{ cacheHitRatio: 0.7 }, T0],
      [{ cacheHitRatio: 0.7 }, T0 + 4 * MIN],
      [{ cacheHitRatio: 0.7 }, T0 + 5 * MIN],
    ]);
    expect(fired).toEqual(["cache"]);
    expect(last.status.cache.kind).toBe("firing");
  });

  it("queda pendiente mientras cumple la espera", () => {
    const { last } = run([[{ cacheHitRatio: 0.7 }, T0]]);
    expect(last.status.cache.kind).toBe("pending");
  });

  it("sin dato de caché no avisa ni resuelve", () => {
    const { fired, resolved, states } = run([
      [{ cacheHitRatio: 0.7 }, T0],
      [{ cacheHitRatio: 0.7 }, T0 + 6 * MIN],
      [{ cacheHitRatio: null }, T0 + 8 * MIN],
    ]);
    expect(fired).toEqual(["cache"]);
    expect(resolved).toEqual([]);
    expect(states.cache.firing).toBe(true);
  });

  it("se resuelve recién cuando sube sobre la banda, no al rozar el umbral", () => {
    const { resolved } = run([
      [{ cacheHitRatio: 0.7 }, T0],
      [{ cacheHitRatio: 0.7 }, T0 + 6 * MIN],
      [{ cacheHitRatio: 0.81 }, T0 + 7 * MIN],
      [{ cacheHitRatio: 0.84 }, T0 + 8 * MIN],
    ]);
    expect(resolved).toEqual(["cache"]);
  });
});

describe("reglas apagadas y estado", () => {
  it("una regla apagada no salta aunque se incumpla", () => {
    const rules = defaultRules();
    rules.blocked.enabled = false;
    const { fired, last } = run([[{ longestBlockedSeconds: 999 }, T0]], rules);
    expect(fired).toEqual([]);
    expect(last.status.blocked.kind).toBe("off");
  });

  it("apagar una regla encendida descarta su estado sin anunciar una resolución", () => {
    let states = run([[{ longestBlockedSeconds: 60 }, T0]]).states;
    expect(states.blocked.firing).toBe(true);
    const rules = allOn();
    rules.blocked.enabled = false;
    const result = evaluateAlerts({ ...calm, longestBlockedSeconds: 60 }, rules, states, T0 + SEC);
    states = result.states;
    expect(result.resolved).toEqual([]);
    expect(states.blocked.firing).toBe(false);
  });

  it("reencender una regla ya incumplida avisa como nueva", () => {
    const off = allOn();
    off.blocked.enabled = false;
    const a = evaluateAlerts({ ...calm, longestBlockedSeconds: 60 }, off, initialStates(), T0);
    const b = evaluateAlerts({ ...calm, longestBlockedSeconds: 60 }, allOn(), a.states, T0 + SEC);
    expect(b.fired.map((x) => x.key)).toEqual(["blocked"]);
  });

  it("varias reglas a la vez son independientes", () => {
    const { fired } = run([
      [{ longestBlockedSeconds: 60, longestIdleInTransactionSeconds: 20 * 60 }, T0],
    ]);
    expect(fired.sort()).toEqual(["blocked", "idleTx"]);
  });

  it("no muta el estado que recibe", () => {
    const states = initialStates();
    const copy = JSON.parse(JSON.stringify(states));
    evaluateAlerts({ ...calm, longestBlockedSeconds: 60 }, allOn(), states, T0);
    expect(states).toEqual(copy);
  });

  it("tolera un estado previo al que le falta una regla", () => {
    const partial = { blocked: initialStates().blocked } as unknown as AlertStates;
    expect(() => evaluateAlerts(calm, allOn(), partial, T0)).not.toThrow();
  });

  it("todo en calma no dice nada", () => {
    const { fired, resolved } = run([
      [{}, T0],
      [{}, T0 + 2 * SEC],
    ]);
    expect(fired).toEqual([]);
    expect(resolved).toEqual([]);
  });
});
