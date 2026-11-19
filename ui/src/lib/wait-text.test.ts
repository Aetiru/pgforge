import { describe, expect, it } from "vitest";
import type { WaitInfo } from "./ipc";
import { describeWait } from "./wait-text";

const base: WaitInfo = { waitEventType: null, waitEvent: null, blockedBy: 0, blocker: null };

describe("describeWait", () => {
  it("con un bloqueador dice quién, en qué estado y desde cuándo", () => {
    const text = describeWait({
      ...base,
      waitEventType: "Lock",
      blockedBy: 1,
      blocker: {
        pid: 4412,
        user: "app",
        state: "idle in transaction",
        transactionSeconds: 540,
        query: "UPDATE clientes\n   SET a = 1",
      },
    });
    expect(text?.blocked).toBe(true);
    expect(text?.summary).toContain("sesión 4412 (app)");
    expect(text?.summary).toContain("sin hacer nada");
    expect(text?.summary).toContain("9 min");
    expect(text?.detail).toBe("UPDATE clientes SET a = 1");
  });

  it("si hay más de un bloqueador lo dice", () => {
    const text = describeWait({ ...base, blockedBy: 3, blocker: { pid: 1, user: null, state: null, transactionSeconds: null, query: null } });
    expect(text?.summary).toContain("y otras 2");
  });

  it("sin permisos para ver al bloqueador igual dice el pid", () => {
    const text = describeWait({ ...base, blockedBy: 1, blocker: { pid: 9, user: null, state: null, transactionSeconds: null, query: null } });
    expect(text?.summary).toBe("Espera un bloqueo de la sesión 9");
    expect(text?.detail).toBeNull();
  });

  it("una espera de disco se dice, una de cliente no", () => {
    expect(describeWait({ ...base, waitEventType: "IO", waitEvent: "DataFileRead" })?.summary).toBe("Esperando el disco");
    expect(describeWait({ ...base, waitEventType: "Client", waitEvent: "ClientRead" })).toBeNull();
    expect(describeWait(null)).toBeNull();
  });
});
