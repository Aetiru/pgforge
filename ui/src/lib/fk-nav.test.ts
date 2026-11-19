import { describe, expect, it } from "vitest";
import { jumpWhere, linkFor, linkLabel } from "./fk-nav";
import type { TableLink } from "./ipc";

const link = (columns: string[], otherColumns: string[], extra: Partial<TableLink> = {}): TableLink => ({
  name: "fk",
  columns,
  otherColumns,
  oid: 1,
  schema: "public",
  table: "clientes",
  ...extra,
});

describe("saltos por clave foránea", () => {
  it("arma el WHERE sobre las columnas del otro lado", () => {
    const where = jumpWhere(link(["cliente_id"], ["id"]), () => "4821");
    expect(where).toBe(`"id" = '4821'`);
  });

  it("una clave compuesta une las condiciones con AND, en orden", () => {
    const values: Record<string, string> = { a: "1", b: "x" };
    expect(jumpWhere(link(["a", "b"], ["k1", "k2"]), (c) => values[c])).toBe(
      `"k1" = '1' AND "k2" = 'x'`,
    );
  });

  it("un valor NULL no lleva a ninguna parte", () => {
    expect(jumpWhere(link(["a", "b"], ["k1", "k2"]), (c) => (c === "a" ? "1" : null))).toBeNull();
  });

  it("las comillas del valor no se salen del literal", () => {
    expect(jumpWhere(link(["n"], ["nombre"]), () => "o'brien")).toBe(`"nombre" = 'o''brien'`);
  });

  it("el nombre de columna se cita", () => {
    expect(jumpWhere(link(["x"], ["Mi Id"]), () => "1")).toBe(`"Mi Id" = '1'`);
  });

  it("elige la clave más corta de las que contienen la columna", () => {
    const largo = link(["a", "b"], ["k1", "k2"], { name: "largo" });
    const corto = link(["a"], ["id"], { name: "corto" });
    expect(linkFor([largo, corto], "a")?.name).toBe("corto");
    expect(linkFor([largo, corto], "z")).toBeNull();
  });

  it("rotula sin el esquema public", () => {
    expect(linkLabel(link(["a"], ["b"]))).toBe("clientes");
    expect(linkLabel(link(["a"], ["b"], { schema: "ventas" }))).toBe("ventas.clientes");
  });
});
