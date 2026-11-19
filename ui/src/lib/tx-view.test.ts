import { describe, expect, it } from "vitest";
import { runElapsed, txElapsed, txIsStale, txSummary } from "./tx-view";

describe("tx-view", () => {
  it("escribe el tiempo en segundos, minutos y horas", () => {
    expect(txElapsed(0, 12_000)).toBe("12 s");
    expect(txElapsed(0, 125_000)).toBe("2 min");
    expect(txElapsed(0, 3_900_000)).toBe("1 h 5 min");
  });

  it("no da tiempos negativos si el reloj retrocede", () => {
    expect(txElapsed(10_000, 5_000)).toBe("0 s");
  });

  it("marca como vieja la transacción a partir de los 60 s", () => {
    expect(txIsStale(0, 59_999)).toBe(false);
    expect(txIsStale(0, 60_000)).toBe(true);
  });

  it("resume sentencias y filas, y omite las filas si no hay", () => {
    expect(txSummary(1, 0)).toBe("1 sentencia");
    expect(txSummary(3, 14)).toBe("3 sentencias · 14 filas");
    expect(txSummary(2, 1)).toBe("2 sentencias · 1 fila");
  });

  it("el cronómetro usa décimas con coma antes del minuto y mm:ss después", () => {
    expect(runElapsed(0, 12_400)).toBe("12,4 s");
    expect(runElapsed(0, 125_000)).toBe("2:05 min");
    expect(runElapsed(5_000, 1_000)).toBe("0,0 s");
  });
});
