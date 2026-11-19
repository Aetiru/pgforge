import { describe, expect, it } from "vitest";
import { MAX_ATTEMPTS, delayBefore, givesUp } from "./reconnect-plan";

describe("reconexión", () => {
  it("la espera crece y se acaba", () => {
    expect(delayBefore(1)).toBe(2);
    expect(delayBefore(5)).toBe(30);
    expect(delayBefore(MAX_ATTEMPTS)).toBe(60);
    expect(delayBefore(MAX_ATTEMPTS + 1)).toBeNull();
  });

  it("no insiste con una contraseña rechazada ni con una clave de host que cambió", () => {
    expect(givesUp({ kind: "database", code: "28P01", message: "x" })).toBe(true);
    expect(givesUp({ kind: "sshHostKey", host: "h", fingerprint: "f", changed: true })).toBe(true);
    expect(givesUp({ kind: "permission", message: "x" })).toBe(true);
  });

  it("sí insiste con un corte de red", () => {
    expect(givesUp({ kind: "disconnected", message: "x" })).toBe(false);
    expect(givesUp({ kind: "other", message: "timeout" })).toBe(false);
    expect(givesUp(null)).toBe(false);
  });
});
