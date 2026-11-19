import { describe, expect, it } from "vitest";
import { formatThousands } from "./number-format";

describe("formatThousands", () => {
  it("agrupa de a tres y usa coma decimal", () => {
    expect(formatThousands("1234567.89")).toBe("1.234.567,89");
    expect(formatThousands("1000")).toBe("1.000");
    expect(formatThousands("-98765432")).toBe("-98.765.432");
    expect(formatThousands("0.5")).toBe("0,5");
    expect(formatThousands("999")).toBe("999");
  });

  it("no toca códigos ni lo que no es un número llano", () => {
    for (const value of ["00123", "007.5", "1e5", "+5", "2024-01-01", "abc", "", "1,5", "1.2.3", "NaN"]) {
      expect(formatThousands(value)).toBe(value);
    }
  });

  it("no pierde decimales largos", () => {
    expect(formatThousands("12345.678901234567")).toBe("12.345,678901234567");
  });
});
