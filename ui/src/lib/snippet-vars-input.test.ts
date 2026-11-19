import { describe, expect, it } from "vitest";
import type { SnippetVariable } from "./snippet-vars";
import { validateVariable } from "./snippet-vars-input";

const variable = (kind: SnippetVariable["kind"]): SnippetVariable => ({ name: "x", default: "", kind });

describe("validateVariable", () => {
  it("una fecha vacía no manda nada y rige la fecha por omisión", () => {
    expect(validateVariable(variable("date"), "")).toEqual({ value: null, error: null });
  });

  it("acepta una fecha y rechaza una imposible", () => {
    expect(validateVariable(variable("date"), "2026-03-05").value).toBe("2026-03-05");
    expect(validateVariable(variable("date"), "2026-13-40").error).toMatch(/fecha válida/);
    expect(validateVariable(variable("date"), "ayer").error).toMatch(/fecha válida/);
  });

  it("el número admite la coma decimal y la pasa a punto", () => {
    expect(validateVariable(variable("number"), "12,5").value).toBe("12.5");
    expect(validateVariable(variable("number"), "-3").value).toBe("-3");
    expect(validateVariable(variable("number"), "1e5").error).toMatch(/número/);
    expect(validateVariable(variable("number"), "").value).toBeNull();
  });

  it("tabla y columna recortan los costados y la lista deja pasar la opción tal cual", () => {
    expect(validateVariable(variable("table"), "  public.pedidos ").value).toBe("public.pedidos");
    expect(validateVariable(variable("column"), " id").value).toBe("id");
    expect(validateVariable(variable("list"), "activo").value).toBe("activo");
  });

  it("un texto vacío sí es un valor", () => {
    expect(validateVariable(variable("text"), "")).toEqual({ value: "", error: null });
  });
});
