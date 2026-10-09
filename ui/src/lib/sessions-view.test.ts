import { describe, expect, it } from "vitest";

import {
  barShare,
  countSessions,
  filterSessions,
  groupSessions,
  isLong,
  kindOf,
  sortSessions,
} from "./sessions-view";
import type { Backend } from "./ipc";

function session(over: Partial<Backend>): Backend {
  return {
    pid: 1,
    database: "ventas",
    user: "app",
    applicationName: "api",
    clientAddr: null,
    backendType: "client backend",
    state: "active",
    waitEventType: null,
    waitEvent: null,
    query: "SELECT 1",
    queryId: null,
    leaderPid: null,
    querySeconds: 1,
    transactionSeconds: 1,
    stateSeconds: 1,
    blockedBy: [],
    isMonitor: false,
    ...over,
  };
}

describe("kindOf", () => {
  it("una sesión que espera un candado es bloqueada aunque esté activa", () => {
    expect(kindOf(session({ state: "active", blockedBy: [9] }))).toBe("blocked");
  });

  it("reconoce la transacción abierta sin trabajar", () => {
    expect(kindOf(session({ state: "idle in transaction" }))).toBe("itx");
    expect(kindOf(session({ state: "idle in transaction (aborted)" }))).toBe("itx");
  });

  it("lo que no se entiende cuenta como inactiva y no desaparece", () => {
    expect(kindOf(session({ state: null }))).toBe("idle");
    expect(kindOf(session({ state: "fastpath function call" }))).toBe("idle");
  });
});

describe("countSessions y filterSessions", () => {
  const list = [
    session({ pid: 1, state: "active" }),
    session({ pid: 2, state: "idle" }),
    session({ pid: 3, state: "active", blockedBy: [1], query: "DELETE FROM pedidos" }),
  ];

  it("cada sesión se cuenta en un solo estado", () => {
    expect(countSessions(list)).toEqual({ all: 3, blocked: 1, itx: 0, active: 1, idle: 1 });
  });

  it("filtra por estado y por texto", () => {
    expect(filterSessions(list, { kind: "blocked", text: "" }).map((s) => s.pid)).toEqual([3]);
    expect(filterSessions(list, { kind: "all", text: "delete" }).map((s) => s.pid)).toEqual([3]);
  });

  it("una columna sin dato no rompe la búsqueda", () => {
    const hidden = session({ pid: 4, user: null, applicationName: null, query: null });
    expect(filterSessions([hidden], { kind: "all", text: "4" })).toHaveLength(1);
  });
});

describe("sortSessions", () => {
  it("desempata por PID para que las filas no salten entre muestras", () => {
    const list = [
      session({ pid: 5, querySeconds: 2 }),
      session({ pid: 3, querySeconds: 2 }),
      session({ pid: 4, querySeconds: 9 }),
    ];
    expect(sortSessions(list, "duration", true).map((s) => s.pid)).toEqual([4, 3, 5]);
  });
});

describe("groupSessions", () => {
  it("agrupa por aplicación con el grupo de la consulta más larga primero", () => {
    const list = [
      session({ pid: 1, applicationName: "a", querySeconds: 1 }),
      session({ pid: 2, applicationName: "b", querySeconds: 50 }),
      session({ pid: 3, applicationName: "a", querySeconds: 2 }),
      session({ pid: 4, applicationName: "", querySeconds: 0 }),
    ];
    const groups = groupSessions(list, "app");
    expect(groups.map((g) => [g.label, g.items.length])).toEqual([
      ["b", 1],
      ["a", 2],
      ["(sin nombre)", 1],
    ]);
  });

  it("sin agrupar devuelve un solo grupo sin rótulo", () => {
    expect(groupSessions([session({})], "none")).toHaveLength(1);
  });
});

describe("barShare e isLong", () => {
  it("la escala logarítmica deja ver lo corto al lado de lo largo", () => {
    expect(barShare(0, 420)).toBe(0);
    expect(barShare(420, 420)).toBe(1);
    expect(barShare(1, 420)).toBeGreaterThan(0.1);
  });

  it("solo resalta lo que está corriendo y supera el umbral", () => {
    expect(isLong(session({ querySeconds: 40 }), 30)).toBe(true);
    expect(isLong(session({ querySeconds: 40, state: "idle" }), 30)).toBe(false);
    expect(isLong(session({ querySeconds: 40 }), 0)).toBe(false);
  });
});
