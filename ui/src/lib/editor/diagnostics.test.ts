import { ChangeSet } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { hasVariables, mapDiagnostics, serverDiagnostic, worstSeverity, type Diagnostic } from "./diagnostics";

const diag = (from: number, to: number): Diagnostic => ({ from, to, severity: "error", message: "x" });

describe("mapDiagnostics", () => {
  it("el subrayado sigue a su palabra cuando se escribe antes", () => {
    const changes = ChangeSet.of({ from: 0, insert: "-- hola\n" }, 20);
    expect(mapDiagnostics([diag(10, 15)], changes)).toEqual([{ ...diag(18, 23) }]);
  });

  it("se va con el texto cuando se lo borra", () => {
    const changes = ChangeSet.of({ from: 8, to: 16 }, 20);
    expect(mapDiagnostics([diag(10, 15)], changes)).toEqual([]);
  });

  it("se acorta si se borra una parte", () => {
    const changes = ChangeSet.of({ from: 12, to: 14 }, 20);
    expect(mapDiagnostics([diag(10, 15)], changes)).toEqual([diag(10, 13)]);
  });
});

describe("serverDiagnostic", () => {
  const sent = "SELECT * FROM clientez WHERE id = 1";

  it("subraya la palabra entera donde dijo el servidor, corrida al lugar de la sentencia", () => {
    const d = serverDiagnostic(sent, 100, {
      status: "error",
      message: 'no existe la relación «clientez»',
      position: 14,
      hint: null,
    });
    expect(d).toMatchObject({ from: 114, to: 122, severity: "error" });
  });

  it("cuenta caracteres y no unidades UTF-16: un emoji antes no corre la marca", () => {
    const text = "SELECT '😀', xx";
    const d = serverDiagnostic(text, 0, { status: "error", message: "m", position: 12, hint: null });
    // El `xx` está en el carácter 12 (base 0) y en la unidad UTF-16 13: el emoji ocupa dos.
    expect(d?.from).toBe(text.indexOf("xx"));
  });

  it("sin posición subraya la primera palabra", () => {
    const d = serverDiagnostic(sent, 0, { status: "error", message: "m", position: null, hint: null });
    expect(sent.slice(d!.from, d!.to)).toBe("SELECT");
  });

  it("ok y skipped no pintan nada", () => {
    expect(serverDiagnostic(sent, 0, { status: "ok" })).toBeNull();
    expect(serverDiagnostic(sent, 0, { status: "skipped", reason: "x" })).toBeNull();
  });

  it("una posición fuera del texto se ignora en vez de romper", () => {
    expect(serverDiagnostic("abc", 0, { status: "error", message: "m", position: 99, hint: null })).toBeNull();
  });
});

describe("otros", () => {
  it("el más grave gana", () => {
    expect(worstSeverity([{ ...diag(0, 1), severity: "info" }, { ...diag(0, 1), severity: "warning" }])).toBe("warning");
    expect(worstSeverity([])).toBeNull();
  });

  it("detecta las variables de pgForge", () => {
    expect(hasVariables("SELECT {{tabla}}")).toBe(true);
    expect(hasVariables("SELECT '{}'")).toBe(false);
  });
});
