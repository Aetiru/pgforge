import { describe, expect, it } from "vitest";
import { defaultTimeout } from "./query-timeout";

describe("defaultTimeout", () => {
  it("en producción sin límite propio pone cinco minutos", () => {
    expect(defaultTimeout("prod", undefined)).toBe(300_000);
    expect(defaultTimeout("prod", 0)).toBe(300_000);
  });

  it("no pisa un límite del perfil ni toca otros entornos", () => {
    expect(defaultTimeout("prod", 30_000)).toBeNull();
    expect(defaultTimeout("dev", undefined)).toBeNull();
    expect(defaultTimeout(null, undefined)).toBeNull();
  });
});
