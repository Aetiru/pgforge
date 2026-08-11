import { describe, expect, it } from "vitest";
import {
  compositeChanges,
  droppedLabels,
  enumChanges,
  type FieldRow,
  type LabelRow,
} from "./type-form";

function label(original: string, value = original): LabelRow {
  return { original, value };
}

function field(original: string, name: string, dataType: string): FieldRow {
  return { original, name, dataType };
}

describe("enumChanges", () => {
  it("no genera nada cuando no se tocó ningún valor", () => {
    const rows = [label("activo"), label("inactivo")];
    expect(enumChanges("public", "estado", ["activo", "inactivo"], rows)).toEqual([]);
  });

  it("ancla el valor nuevo después del anterior para respetar el orden de la pantalla", () => {
    const rows = [label("activo"), label("", "pausado"), label("inactivo")];
    const changes = enumChanges("public", "estado", ["activo", "inactivo"], rows);

    expect(changes).toEqual([
      {
        kind: "addEnumValue",
        schema: "public",
        name: "estado",
        value: "pausado",
        position: { kind: "after", value: "activo" },
        ifNotExists: true,
      },
    ]);
  });

  it("un valor nuevo al principio va sin ancla y queda donde lo ponga el servidor", () => {
    const rows = [label("", "pendiente"), label("activo")];
    const changes = enumChanges("public", "estado", ["activo"], rows);

    expect(changes[0]).toMatchObject({ kind: "addEnumValue", position: null });
  });

  it("renombra el valor que cambió de texto", () => {
    const rows = [label("activo", "vigente"), label("inactivo")];
    const changes = enumChanges("public", "estado", ["activo", "inactivo"], rows);

    expect(changes).toEqual([
      {
        kind: "renameEnumValue",
        schema: "public",
        name: "estado",
        from: "activo",
        to: "vigente",
      },
    ]);
  });

  it("ignora las filas vacías", () => {
    const rows = [label("activo"), label("", "   ")];
    expect(enumChanges("public", "estado", ["activo"], rows)).toEqual([]);
  });

  it("informa los valores que se sacaron de la lista, porque no se pueden borrar", () => {
    const rows = [label("activo")];
    expect(droppedLabels(["activo", "inactivo"], rows)).toEqual(["inactivo"]);
    expect(droppedLabels(["activo"], rows)).toEqual([]);
  });
});

describe("compositeChanges", () => {
  const before = [
    { name: "calle", dataType: "text", collation: null },
    { name: "numero", dataType: "integer", collation: null },
  ];

  it("no genera nada cuando no se tocó ningún campo", () => {
    const rows = [field("calle", "calle", "text"), field("numero", "numero", "integer")];
    expect(compositeChanges("public", "direccion", before, rows)).toEqual([]);
  });

  it("agrega el campo nuevo", () => {
    const rows = [
      field("calle", "calle", "text"),
      field("numero", "numero", "integer"),
      field("", "piso", "text"),
    ];

    expect(compositeChanges("public", "direccion", before, rows)).toEqual([
      {
        kind: "addCompositeField",
        schema: "public",
        name: "direccion",
        field: { name: "piso", dataType: "text", collation: null },
      },
    ]);
  });

  it("cambia el tipo del campo que ya estaba", () => {
    const rows = [field("calle", "calle", "text"), field("numero", "numero", "bigint")];

    expect(compositeChanges("public", "direccion", before, rows)).toEqual([
      {
        kind: "alterCompositeFieldType",
        schema: "public",
        name: "direccion",
        field: "numero",
        dataType: "bigint",
        collation: null,
        cascade: false,
      },
    ]);
  });

  it("borra el campo que desapareció de la pantalla", () => {
    const rows = [field("calle", "calle", "text")];

    expect(compositeChanges("public", "direccion", before, rows)).toEqual([
      {
        kind: "dropCompositeField",
        schema: "public",
        name: "direccion",
        field: "numero",
        cascade: false,
      },
    ]);
  });

  it("una fila sin nombre o sin tipo no genera nada", () => {
    const rows = [
      field("calle", "calle", "text"),
      field("numero", "numero", "integer"),
      field("", "piso", "  "),
      field("", "", "text"),
    ];
    expect(compositeChanges("public", "direccion", before, rows)).toEqual([]);
  });
});
