import { describe, expect, it } from "vitest";
import { TtlCache } from "./ttl-cache";

describe("TtlCache", () => {
  it("pide una sola vez mientras no venza y comparte la promesa en vuelo", async () => {
    let clock = 0;
    let calls = 0;
    const cache = new TtlCache<number>(1000, () => clock);
    const load = async () => ++calls;
    const [a, b] = await Promise.all([cache.get("k", load), cache.get("k", load)]);
    expect([a, b, calls]).toEqual([1, 1, 1]);

    clock = 999;
    expect(await cache.get("k", load)).toBe(1);
    clock = 1000;
    expect(await cache.get("k", load)).toBe(2);
  });

  it("olvida un pedido que falló, para reintentar en la próxima", async () => {
    const cache = new TtlCache<number>(1000);
    await expect(cache.get("k", () => Promise.reject(new Error("x")))).rejects.toThrow();
    await Promise.resolve();
    expect(await cache.get("k", async () => 7)).toBe(7);
  });

  it("no crece sin techo", async () => {
    const cache = new TtlCache<number>(1000, Date.now, 2);
    await cache.get("a", async () => 1);
    await cache.get("b", async () => 2);
    await cache.get("c", async () => 3);
    let calls = 0;
    await cache.get("a", async () => ++calls);
    expect(calls).toBe(1);
  });
});
