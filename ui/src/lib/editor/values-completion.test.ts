import { describe, expect, it } from "vitest";
import { frequencyLabel, valueInsertText, valueKind, valueOptions } from "./values-completion";

describe("valueKind", () => {
  it("separa números, booleanos y texto", () => {
    expect(valueKind("integer")).toBe("number");
    expect(valueKind("numeric(12,2)")).toBe("number");
    expect(valueKind("double precision")).toBe("number");
    expect(valueKind("boolean")).toBe("boolean");
    expect(valueKind("text")).toBe("text");
    expect(valueKind("estado_pedido")).toBe("text");
    expect(valueKind(undefined)).toBe("text");
  });
});

describe("valueInsertText", () => {
  const text = { kind: "text" as const, quoteFollows: false };

  it("sin comilla abierta, escribe el literal entero y escapa las comillas", () => {
    expect(valueInsertText("activo", { insideQuote: false, ...text })).toBe("'activo'");
    expect(valueInsertText("O'Brien", { insideQuote: false, ...text })).toBe("'O''Brien'");
  });

  it("dentro de una comilla cierra solo si nadie la cerró", () => {
    expect(valueInsertText("activo", { insideQuote: true, ...text })).toBe("activo'");
    expect(valueInsertText("activo", { insideQuote: true, kind: "text", quoteFollows: true })).toBe("activo");
    expect(valueInsertText("O'Brien", { insideQuote: true, kind: "text", quoteFollows: true })).toBe("O''Brien");
  });

  it("números y booleanos van sin comillas, y t/f se vuelven true/false", () => {
    expect(valueInsertText("42", { insideQuote: false, kind: "number", quoteFollows: false })).toBe("42");
    expect(valueInsertText("t", { insideQuote: false, kind: "boolean", quoteFollows: false })).toBe("true");
    expect(valueInsertText("f", { insideQuote: false, kind: "boolean", quoteFollows: false })).toBe("false");
  });
});

describe("valueOptions", () => {
  it("none no ofrece nada", () => {
    expect(valueOptions({ kind: "none", values: [] }, "text", false, false)).toEqual([]);
  });

  it("muestra la frecuencia como porcentaje y conserva el orden", () => {
    const options = valueOptions(
      {
        kind: "frequent",
        values: [
          { value: "activo", frequency: 0.62 },
          { value: "baja", frequency: 0.004 },
        ],
      },
      "text",
      false,
      false,
    );
    expect(options.map((o) => [o.label, o.detail, o.apply])).toEqual([
      ["activo", "62 %", "'activo'"],
      ["baja", "< 1 %", "'baja'"],
    ]);
    expect(options[0].boost).toBeGreaterThan(options[1].boost ?? 0);
  });

  it("una etiqueta de enum no lleva porcentaje", () => {
    expect(frequencyLabel(null)).toBeUndefined();
    const [option] = valueOptions({ kind: "enum", values: [{ value: "x", frequency: null }] }, "text", false, false);
    expect(option.detail).toBeUndefined();
    expect(option.type).toBe("enum");
  });
});
