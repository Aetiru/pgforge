import { describe, expect, it } from "vitest";
import { blankColumn, tableChange, validateTable, type DraftColumn } from "./table-form";

function draft(name: string, typeName: string): DraftColumn {
  return { ...blankColumn(), name, typeName };
}

describe("validateTable", () => {
  it("pide nombre y al menos una columna", () => {
    expect(validateTable("  ", [draft("id", "bigint")])).toBe("Poné un nombre para la tabla.");
    expect(validateTable("clientes", [])).toBe("Una tabla necesita al menos una columna.");
    expect(validateTable("clientes", [draft("id", "bigint")])).toBeNull();
  });

  it("señala cuál de las columnas está a medias", () => {
    expect(validateTable("clientes", [draft("id", "bigint"), draft("nombre", " ")])).toBe(
      "La columna nombre necesita un tipo.",
    );
    expect(validateTable("clientes", [draft(" ", "text")])).toBe(
      "Todas las columnas necesitan un nombre.",
    );
  });
});

describe("tableChange", () => {
  it("la clave de la fila del formulario no viaja al núcleo", () => {
    const change = tableChange("public", " clientes ", [draft("id", "bigint")]);
    expect(change).toEqual({
      kind: "createTable",
      schema: "public",
      name: "clientes",
      columns: [{ name: "id", typeName: "bigint", notNull: false, default: null, identity: null }],
    });
  });
});
