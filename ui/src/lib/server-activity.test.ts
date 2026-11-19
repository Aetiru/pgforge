import { describe, expect, it } from "vitest";
import { activityBadge } from "./server-activity";

const brief = (over = {}) => ({ blocked: 0, longestIdleInTransactionSeconds: null, restricted: false, ...over });

describe("activityBadge", () => {
  it("sin nada que decir no hay insignia", () => {
    expect(activityBadge(undefined)).toBeNull();
    expect(activityBadge(brief())).toBeNull();
    expect(activityBadge(brief({ longestIdleInTransactionSeconds: 30 }))).toBeNull();
  });

  it("cuenta las bloqueadas y se agrava con muchas", () => {
    expect(activityBadge(brief({ blocked: 1 }))).toMatchObject({ text: "1 bloqueada", level: "warn" });
    expect(activityBadge(brief({ blocked: 7 }))).toMatchObject({ text: "7 bloqueadas", level: "bad" });
  });

  it("una transacción inactiva larga se dice con su duración", () => {
    expect(activityBadge(brief({ longestIdleInTransactionSeconds: 2400 }))).toMatchObject({
      text: "transacción de 40 min",
      level: "warn",
    });
    expect(activityBadge(brief({ longestIdleInTransactionSeconds: 7200 }))?.level).toBe("bad");
  });

  it("las bloqueadas ganan sobre la transacción", () => {
    expect(activityBadge(brief({ blocked: 2, longestIdleInTransactionSeconds: 9999 }))?.text).toBe("2 bloqueadas");
  });

  it("sin permiso para ver las demás sesiones no afirma nada", () => {
    expect(activityBadge(brief({ blocked: 3, restricted: true }))).toBeNull();
  });
});
