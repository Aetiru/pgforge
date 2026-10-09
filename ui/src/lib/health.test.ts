import { describe, expect, it } from "vitest";

import { assessHealth, connectionLevel } from "./health";
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
};

describe("assessHealth", () => {
  it("un servidor tranquilo no tiene nada que decir", () => {
    expect(assessHealth(calm)).toEqual({ level: "ok", issues: [] });
  });

  it("una sesión esperando a otra es lo más grave", () => {
    const health = assessHealth({ ...calm, waitingConnections: 2 });
    expect(health.level).toBe("bad");
    expect(health.issues[0].text).toBe("2 sesiones esperan a otra");
  });

  it("una transacción inactiva avisa pero no alarma", () => {
    const health = assessHealth({ ...calm, idleInTransaction: 1 });
    expect(health.level).toBe("warn");
  });

  it("el caché bajo es solo un aviso, y sin dato no dice nada", () => {
    expect(assessHealth({ ...calm, cacheHitRatio: 0.5 }).level).toBe("warn");
    expect(assessHealth({ ...calm, cacheHitRatio: null }).level).toBe("ok");
  });

  it("la transacción más vieja sube de nivel con las horas", () => {
    expect(assessHealth({ ...calm, longestTransactionSeconds: 400 }).level).toBe("warn");
    expect(assessHealth({ ...calm, longestTransactionSeconds: 4000 }).level).toBe("bad");
  });
});

describe("connectionLevel", () => {
  it("sin máximo configurado no opina", () => {
    expect(connectionLevel(50, 0)).toBe("ok");
  });

  it("sube a medida que se llena la tabla", () => {
    expect([50, 71, 91].map((n) => connectionLevel(n, 100))).toEqual(["ok", "warn", "bad"]);
  });
});
