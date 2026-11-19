import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { relativeTime, versionKey, VersionScheduler, worthSaving, type VersionedTab } from "./versions";

const tab = (over: Partial<VersionedTab> = {}): VersionedTab => ({
  key: "tab-3",
  profileId: "p1",
  scriptPath: null,
  filePath: null,
  savedId: null,
  ...over,
});

describe("clave de la pestaña", () => {
  it("prefiere lo que sobrevive a un reinicio", () => {
    expect(versionKey(tab({ scriptPath: "a/b.sql", filePath: "/x.sql", savedId: 4 }))).toBe(
      "script:p1:a/b.sql",
    );
    expect(versionKey(tab({ filePath: "/x.sql", savedId: 4 }))).toBe("file:/x.sql");
    expect(versionKey(tab({ savedId: 4 }))).toBe("saved:4");
    expect(versionKey(tab())).toBe("tab:p1:tab-3");
  });

  it("el mismo script en dos servidores no comparte copias", () => {
    expect(versionKey(tab({ scriptPath: "a.sql" }))).not.toBe(
      versionKey(tab({ profileId: "p2", scriptPath: "a.sql" })),
    );
  });
});

describe("cuándo vale la pena guardar", () => {
  it("nunca vacío ni igual al último", () => {
    expect(worthSaving("  \n", null)).toBe(false);
    expect(worthSaving("select 1", "select 1")).toBe(false);
    expect(worthSaving("select 2", "select 1")).toBe(true);
    expect(worthSaving("select 1", undefined)).toBe(true);
  });
});

describe("planificador de copias", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("guarda tras la quietud y no antes", () => {
    const save = vi.fn().mockResolvedValue(1);
    const scheduler = new VersionScheduler(save, 90_000, {
      set: (cb, ms) => setTimeout(cb, ms),
      clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
    });
    let text = "select 1";
    scheduler.touch("k", () => text);
    vi.advanceTimersByTime(60_000);
    text = "select 12";
    scheduler.touch("k", () => text); // teclear reinicia la cuenta
    vi.advanceTimersByTime(60_000);
    expect(save).not.toHaveBeenCalled();
    vi.advanceTimersByTime(30_000);
    expect(save).toHaveBeenCalledWith("k", "select 12");
  });

  it("no repite un texto ya guardado", () => {
    const save = vi.fn().mockResolvedValue(1);
    const scheduler = new VersionScheduler(save);
    expect(scheduler.saveNow("k", "select 1")).toBe(true);
    expect(scheduler.saveNow("k", "select 1")).toBe(false);
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("un guardado que falla se puede reintentar con el mismo texto", async () => {
    const save = vi.fn().mockRejectedValueOnce(new Error("x")).mockResolvedValue(1);
    const scheduler = new VersionScheduler(save);
    scheduler.saveNow("k", "select 1");
    await Promise.resolve();
    await Promise.resolve();
    expect(scheduler.saveNow("k", "select 1")).toBe(true);
  });

  it("cerrar guarda lo pendiente y cancela el temporizador", () => {
    const save = vi.fn().mockResolvedValue(1);
    const scheduler = new VersionScheduler(save);
    scheduler.touch("k", () => "select 1");
    scheduler.close("k", "select 1");
    expect(save).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(200_000);
    expect(save).toHaveBeenCalledTimes(1);
  });

  it("cerrar con la pestaña vacía no guarda nada", () => {
    const save = vi.fn().mockResolvedValue(1);
    new VersionScheduler(save).close("k", "   ");
    expect(save).not.toHaveBeenCalled();
  });

  it("tras olvidar la clave se puede volver a guardar el mismo texto", () => {
    const save = vi.fn().mockResolvedValue(1);
    const scheduler = new VersionScheduler(save);
    scheduler.saveNow("k", "select 1");
    scheduler.forget("k");
    expect(scheduler.saveNow("k", "select 1")).toBe(true);
  });
});

describe("hora relativa", () => {
  it("usa la escala que importa", () => {
    const now = 1_000_000;
    expect(relativeTime(now - 10, now)).toBe("hace un momento");
    expect(relativeTime(now - 20 * 60, now)).toBe("hace 20 min");
    expect(relativeTime(now - 3 * 3600, now)).toBe("hace 3 h");
    expect(relativeTime(now - 86_400, now)).toBe("ayer");
    expect(relativeTime(now - 5 * 86_400, now)).toBe("hace 5 días");
    expect(relativeTime(now + 50, now)).toBe("hace un momento");
  });
});
