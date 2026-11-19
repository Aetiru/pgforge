import { describe, expect, it } from "vitest";
import { daysText, growthText, levelTag, millions, worstLevel } from "./diagnose-view";

describe("diagnose-view", () => {
  it("elige la clase de cada nivel", () => {
    expect(levelTag("attention")).toBe("tag-bad");
    expect(levelTag("review")).toBe("tag-warn");
    expect(levelTag("info")).toBe("tag-info");
    expect(levelTag("ok")).toBe("tag-ok");
  });

  it("el peor nivel gana y sin niveles está en orden", () => {
    expect(worstLevel(["ok", "review", "info"])).toBe("review");
    expect(worstLevel(["attention", "review"])).toBe("attention");
    expect(worstLevel([])).toBe("ok");
  });

  it("dice los plazos a la escala que importa", () => {
    expect(daysText(0.2)).toBe("menos de un día");
    expect(daysText(1)).toBe("1 día");
    expect(daysText(12.4)).toBe("12 días");
    expect(daysText(150)).toBe("5 meses");
    expect(daysText(1100)).toBe("unos 3 años");
    expect(daysText(100000)).toBe("más de 20 años");
    expect(daysText(null)).toBe("—");
  });

  it("escribe el crecimiento con signo", () => {
    expect(growthText(null)).toBe("faltan muestras");
    expect(growthText(2.1 * 1024 ** 3)).toBe("+2,1 GB/día");
    expect(growthText(-(1024 ** 2))).toBe("−1,0 MB/día");
  });

  it("escribe la edad en millones", () => {
    expect(millions(1_480_000_000)).toContain("480 M");
  });
});
