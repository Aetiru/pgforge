import { describe, expect, it } from "vitest";

import { CHECK_INTERVAL_MS, shouldCheck } from "./update.svelte";

/**
 * Lo que se prueba es cuándo se vuelve a preguntar. El resto del cartel —traer la release, abrir el
 * navegador— es la frontera del IPC y no tiene lógica propia.
 */
describe("cada cuánto se pregunta por una versión nueva", () => {
  const AHORA = Date.parse("2026-08-13T12:00:00Z");

  it("la primera vez pregunta", () => {
    expect(shouldCheck(null, AHORA)).toBe(true);
  });

  it("no vuelve a preguntar el mismo día", () => {
    expect(shouldCheck(AHORA - 60_000, AHORA)).toBe(false);
    expect(shouldCheck(AHORA - CHECK_INTERVAL_MS + 1, AHORA)).toBe(false);
  });

  it("pasado el intervalo pregunta de nuevo", () => {
    expect(shouldCheck(AHORA - CHECK_INTERVAL_MS, AHORA)).toBe(true);
  });

  it("un reloj corrido hacia atrás no deja el aviso mudo para siempre", () => {
    expect(shouldCheck(AHORA + CHECK_INTERVAL_MS, AHORA)).toBe(true);
  });
});
