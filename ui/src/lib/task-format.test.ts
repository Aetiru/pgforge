import { describe, expect, it } from "vitest";
import { elapsedText, outcomeText, progressText, taskKindLabel } from "./task-format";

describe("outcomeText", () => {
  it("cuenta solo el tiempo cuando el proceso no movió datos", () => {
    expect(outcomeText({ kind: "maintenance", seconds: 12.5 })).toBe("Terminó: en 12.5 s.");
  });

  it("suma las filas y los bytes de una importación", () => {
    expect(outcomeText({ kind: "import", seconds: 3, bytes: 2048, rows: 1_500_000 })).toBe(
      "Terminó: 1.500.000 filas, 2.0 kB, en 3.0 s.",
    );
  });

  it("avisa de los errores que pg_restore ignoró", () => {
    expect(outcomeText({ kind: "restore", seconds: 61, ignoredErrors: 3 })).toBe(
      "Terminó: 3 errores ignorados, en 1 min 1 s.",
    );
  });

  it("no menciona los errores ignorados cuando no hubo ninguno", () => {
    expect(outcomeText({ kind: "restore", seconds: 2, ignoredErrors: 0 })).toBe(
      "Terminó: en 2.0 s.",
    );
  });

  it("distingue cero filas de que no se informen filas", () => {
    expect(outcomeText({ kind: "import", seconds: 1, rows: 0 })).toBe("Terminó: 0 filas, en 1.0 s.");
    expect(outcomeText({ kind: "export", seconds: 1 })).toBe("Terminó: en 1.0 s.");
  });
});

describe("elapsedText", () => {
  it("cuenta contra ahora mientras corre", () => {
    expect(elapsedText(1_000, null, 61_000)).toBe("1 min 0 s");
  });

  it("se congela en lo que tardó una vez terminado", () => {
    expect(elapsedText(1_000, 6_000, 999_000)).toBe("5.0 s");
  });

  it("no muestra tiempos negativos si el reloj se corrió", () => {
    expect(elapsedText(5_000, null, 1_000)).toBe("0 ms");
  });
});

describe("progressText y taskKindLabel", () => {
  it("cuenta el avance en bytes legibles", () => {
    expect(progressText(1_048_576)).toBe("1.0 MB copiados");
  });

  it("rotula cada clase de proceso", () => {
    expect(taskKindLabel("maintenance")).toBe("Mantenimiento");
    expect(taskKindLabel("index")).toBe("Índice");
  });
});
