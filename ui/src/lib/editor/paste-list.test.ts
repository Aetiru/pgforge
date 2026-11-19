import { describe, expect, it } from "vitest";
import { isPasteAsListKey, PASTE_INTENT_MS, PasteIntent } from "./paste-list";

const key = (over: Partial<Parameters<typeof isPasteAsListKey>[0]> = {}) => ({
  key: "V",
  ctrlKey: true,
  metaKey: false,
  shiftKey: true,
  altKey: false,
  ...over,
});

describe("PasteIntent", () => {
  it("se consume una sola vez", () => {
    const intent = new PasteIntent();
    intent.arm(1000);
    expect(intent.consume(1100)).toBe(true);
    expect(intent.consume(1200)).toBe(false);
  });

  it("sin armar no hay intención: el pegado común no se toca", () => {
    expect(new PasteIntent().consume(5000)).toBe(false);
  });

  it("vence si el evento paste no llega", () => {
    const intent = new PasteIntent();
    intent.arm(1000);
    expect(intent.consume(1000 + PASTE_INTENT_MS + 1)).toBe(false);
  });
});

describe("isPasteAsListKey", () => {
  it("reconoce Ctrl+Mayús+V con la tecla en mayúscula o minúscula", () => {
    expect(isPasteAsListKey(key())).toBe(true);
    expect(isPasteAsListKey(key({ key: "v" }))).toBe(true);
    expect(isPasteAsListKey(key({ ctrlKey: false, metaKey: true }))).toBe(true);
  });

  it("no confunde un Ctrl+V común ni otras combinaciones", () => {
    expect(isPasteAsListKey(key({ shiftKey: false }))).toBe(false);
    expect(isPasteAsListKey(key({ altKey: true }))).toBe(false);
    expect(isPasteAsListKey(key({ key: "c" }))).toBe(false);
    expect(isPasteAsListKey(key({ ctrlKey: false }))).toBe(false);
  });
});
