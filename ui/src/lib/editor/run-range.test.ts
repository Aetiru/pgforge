import { describe, expect, it } from "vitest";
import { rangeFrom, rangeUntil } from "../sql-outline";
import { rangeToRun } from "./run-range";

const script = "SELECT 1;\n\nUPDATE t SET a = 1;\nSELECT 'ñ😀';\nDELETE FROM t;";

describe("rangeToRun", () => {
  it("«hasta» corre desde el principio hasta el final de la sentencia del cursor", () => {
    const at = script.indexOf("UPDATE") + 3;
    const range = rangeToRun(script, at, "until")!;
    expect(range.text).toBe(rangeUntil(script, at));
    expect(range.text.endsWith("a = 1;")).toBe(true);
    expect(range.base).toBe(0);
  });

  it("«desde» empieza en el inicio de la sentencia y avisa dónde", () => {
    const at = script.indexOf("UPDATE") + 3;
    const range = rangeToRun(script, at, "from")!;
    expect(range.text).toBe(rangeFrom(script, at));
    expect(range.text.startsWith("UPDATE")).toBe(true);
    expect(range.base).toBe(script.indexOf("UPDATE"));
  });

  it("el desplazamiento cuenta caracteres Unicode, no unidades UTF-16", () => {
    const at = script.indexOf("DELETE");
    const range = rangeToRun(script, at, "from")!;
    // El emoji ocupa dos unidades en JavaScript y una para el servidor.
    expect(range.base).toBe([...script.slice(0, at)].length);
    expect(range.base).toBeLessThan(at);
  });

  it("sin sentencia aplicable devuelve null", () => {
    expect(rangeToRun("   ", 1, "until")).toBeNull();
    expect(rangeToRun("SELECT 1;\n\n", 11, "from")).toBeNull();
  });
});
