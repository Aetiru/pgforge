import { describe, expect, it } from "vitest";
import { isPlainIdent, isReserved, quoteIdent } from "./sql-ident";

describe("quoteIdent", () => {
  it("deja sin comillas lo que el servidor lee tal cual", () => {
    expect(quoteIdent("cliente_id")).toBe("cliente_id");
    expect(quoteIdent("_x1")).toBe("_x1");
    expect(quoteIdent("name")).toBe("name");
  });

  it("cita mayúsculas, espacios, símbolos y empiezo con dígito", () => {
    expect(quoteIdent("Total")).toBe('"Total"');
    expect(quoteIdent("razón social")).toBe('"razón social"');
    expect(quoteIdent("2024")).toBe('"2024"');
    expect(quoteIdent("a-b")).toBe('"a-b"');
  });

  it("cita las palabras reservadas, en cualquier combinación de mayúsculas", () => {
    expect(quoteIdent("order")).toBe('"order"');
    expect(quoteIdent("user")).toBe('"user"');
    expect(isReserved("SELECT")).toBe(true);
    expect(isReserved("estado")).toBe(false);
  });

  it("duplica las comillas de adentro", () => {
    expect(quoteIdent('a"b')).toBe('"a""b"');
  });

  it("isPlainIdent no acepta lo reservado", () => {
    expect(isPlainIdent("group")).toBe(false);
    expect(isPlainIdent("grupo")).toBe(true);
  });
});
