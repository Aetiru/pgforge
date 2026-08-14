import { describe, expect, it } from "vitest";
import { indexDef, indexForm, toggleColumn, validateIndex } from "./index-form";

const TARGET = { schema: "public", table: "facturas" };

describe("validateIndex", () => {
  it("un índice sin columnas no es un índice", () => {
    expect(validateIndex(indexForm())).toBe("Elegí al menos una columna.");
    expect(validateIndex({ ...indexForm(), columns: ["fecha"] })).toBeNull();
  });
});

describe("indexDef", () => {
  it("lo que se deja en blanco viaja como null: es «que lo decida Postgres»", () => {
    const def = indexDef({ ...indexForm(), columns: ["fecha"] }, TARGET);
    expect(def).toEqual({
      schema: "public",
      table: "facturas",
      name: null,
      unique: false,
      method: null,
      columns: ["fecha"],
      whereClause: null,
      concurrently: false,
    });
  });

  it("el predicado parcial viaja tal como se escribió, sin los espacios de los bordes", () => {
    const form = { ...indexForm(), columns: ["fecha"], whereClause: "  estado = 'activo' " };
    expect(indexDef(form, TARGET).whereClause).toBe("estado = 'activo'");
  });
});

describe("toggleColumn", () => {
  it("conserva el orden en que se fueron marcando: es el orden del índice", () => {
    let columns = toggleColumn([], "fecha");
    columns = toggleColumn(columns, "cliente");
    expect(columns).toEqual(["fecha", "cliente"]);

    columns = toggleColumn(columns, "fecha");
    expect(columns).toEqual(["cliente"]);
  });
});
