import { describe, expect, it } from "vitest";
import { columnChanges, columnForm, validateColumn } from "./column-form";
import type { TableColumn } from "./ipc";

const TARGET = { schema: "public", table: "clientes" };

const original: TableColumn = {
  name: "nombre",
  typeName: "text",
  notNull: false,
  default: null,
  generated: false,
  comment: null,
};

describe("validateColumn", () => {
  it("pide nombre y tipo", () => {
    expect(validateColumn(columnForm(null))).toBe("Poné un nombre para la columna.");
    expect(validateColumn({ ...columnForm(null), name: "edad" })).toBe(
      "Poné un tipo para la columna.",
    );
    expect(validateColumn({ ...columnForm(null), name: "edad", typeName: "integer" })).toBeNull();
  });
});

describe("columnChanges al dar de alta", () => {
  it("manda una sola sentencia con lo escrito", () => {
    const form = { ...columnForm(null), name: " edad ", typeName: " integer ", notNull: true };
    expect(columnChanges(form, TARGET, null)).toEqual([
      {
        kind: "addColumn",
        schema: "public",
        table: "clientes",
        column: {
          name: "edad",
          typeName: "integer",
          notNull: true,
          default: null,
          identity: null,
        },
      },
    ]);
  });

  it("una columna de identidad no lleva default: la llena el servidor", () => {
    const form = {
      ...columnForm(null),
      name: "id",
      typeName: "bigint",
      identity: "always" as const,
      default: "42",
    };
    const [change] = columnChanges(form, TARGET, null);
    expect(change).toMatchObject({ column: { identity: "always", default: null } });
  });
});

describe("columnChanges al editar", () => {
  it("sin tocar nada no manda nada", () => {
    expect(columnChanges(columnForm(original), TARGET, original)).toEqual([]);
  });

  it("el renombre va primero y lo que sigue habla del nombre nuevo", () => {
    const form = { ...columnForm(original), name: "razon_social", typeName: "varchar(120)" };
    const changes = columnChanges(form, TARGET, original);

    expect(changes[0]).toMatchObject({ kind: "renameColumn", column: "nombre", newName: "razon_social" });
    expect(changes[1]).toMatchObject({ kind: "alterColumnType", column: "razon_social" });
  });

  it("el USING viaja solo con el cambio de tipo", () => {
    const form = { ...columnForm(original), typeName: "integer", using: "nombre::integer" };
    expect(columnChanges(form, TARGET, original)).toEqual([
      {
        kind: "alterColumnType",
        schema: "public",
        table: "clientes",
        column: "nombre",
        typeName: "integer",
        using: "nombre::integer",
      },
    ]);
  });

  it("borrar el default manda null y no una cadena vacía", () => {
    const conDefault = { ...original, default: "'sin nombre'" };
    const form = { ...columnForm(conDefault), default: "  " };
    expect(columnChanges(form, TARGET, conDefault)).toEqual([
      {
        kind: "setColumnDefault",
        schema: "public",
        table: "clientes",
        column: "nombre",
        default: null,
      },
    ]);
  });

  it("un default que solo cambió de espacios no cuenta como cambio", () => {
    const conDefault = { ...original, default: "now()" };
    const form = { ...columnForm(conDefault), default: " now() " };
    expect(columnChanges(form, TARGET, conDefault)).toEqual([]);
  });
});
