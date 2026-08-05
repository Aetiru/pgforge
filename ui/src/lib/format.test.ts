import { describe, expect, it } from "vitest";

import { ago, bytes, count, decimal, duration, oneLine, percent } from "./format";

/**
 * Los formatos del dashboard se leen de un vistazo, y el caso que importa es el borde: un cero, un
 * null y el salto de unidad. Ahí es donde una tabla de tamaños empieza a mentir.
 */

describe("bytes", () => {
  it("salta de unidad y deja un decimal solo mientras el número es chico", () => {
    expect(bytes(0)).toBe("0 B");
    expect(bytes(512)).toBe("512 B");
    expect(bytes(1024)).toBe("1.0 kB");
    expect(bytes(1536)).toBe("1.5 kB");
    expect(bytes(20 * 1024)).toBe("20 kB");
    expect(bytes(3.5 * 1024 ** 3)).toBe("3.5 GB");
  });

  it("marca la ausencia de dato con una raya, distinta de un cero", () => {
    expect(bytes(null)).toBe("—");
    expect(bytes(undefined)).toBe("—");
    expect(bytes(0)).not.toBe("—");
  });
});

describe("duration", () => {
  it("elige la unidad por el orden de magnitud, que es lo que se mira", () => {
    expect(duration(0.25)).toBe("250 ms");
    expect(duration(12.34)).toBe("12.3 s");
    expect(duration(90)).toBe("1 min 30 s");
    expect(duration(5400)).toBe("1 h 30 min");
    expect(duration(200000)).toBe("2 d");
  });

  it("marca la ausencia de dato con una raya", () => {
    expect(duration(null)).toBe("—");
  });
});

describe("ago", () => {
  it("dice «nunca» cuando no hay instante, no «hace —»", () => {
    expect(ago(null)).toBe("nunca");
    expect(ago(90)).toBe("hace 1 min 30 s");
  });
});

describe("count, percent y decimal", () => {
  it("cuentan en el formato local y no confunden cero con vacío", () => {
    expect(count(1234567)).toBe((1234567).toLocaleString("es"));
    expect(count(0)).toBe("0");
    expect(count(null)).toBe("—");
  });

  it("expresan la razón como porcentaje con los decimales pedidos", () => {
    expect(percent(0.9876)).toBe("98.8 %");
    expect(percent(0.9876, 0)).toBe("99 %");
    expect(percent(null)).toBe("—");
  });

  it("recortan los decimales sin perder el cero", () => {
    expect(decimal(3.14159, 2)).toBe("3.14");
    expect(decimal(0)).toBe("0.0");
    expect(decimal(undefined)).toBe("—");
  });
});

describe("oneLine", () => {
  it("aplasta la consulta a una línea para que entre en una celda", () => {
    expect(oneLine("SELECT 1\n  FROM  t\n WHERE x")).toBe("SELECT 1 FROM t WHERE x");
    expect(oneLine("   ")).toBe("");
    expect(oneLine(null)).toBe("");
  });

  it("corta con puntos suspensivos y no a la mitad de la celda", () => {
    expect(oneLine("abcdefghij", 4)).toBe("abcd…");
    expect(oneLine("abcd", 4)).toBe("abcd");
  });
});
