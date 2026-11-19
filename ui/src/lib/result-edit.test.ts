import { describe, expect, it } from "vitest";
import type { TableShape } from "./ipc";
import { buildChanges, editedRows, planEdit, withEdit } from "./result-edit";

const col = (name: string, generated = false) => ({
  name,
  typeName: "text",
  notNull: false,
  default: null,
  generated,
  comment: null,
});

const shape = (over: Partial<TableShape> = {}): TableShape => ({
  oid: 1,
  schema: "public",
  name: "clientes",
  columns: [col("id", true), col("nombre"), col("estado")],
  key: { name: "clientes_pkey", kind: "primary", columns: ["id"] },
  readOnly: null,
  foreignKeys: [],
  referencedBy: [],
  ...over,
});

describe("planEdit", () => {
  it("con la clave en el resultado se puede editar lo que no calcula el servidor", () => {
    const plan = planEdit(shape(), { oid: 1, columns: ["id", "nombre", null] });
    expect(typeof plan).toBe("object");
    if (typeof plan === "string") return;
    expect(plan.keyAt).toEqual([0]);
    expect(plan.editable).toEqual([false, true, false]);
  });

  it("sin la clave en el resultado explica qué agregar", () => {
    const plan = planEdit(shape(), { oid: 1, columns: ["nombre", "estado"] });
    expect(plan).toMatch(/«id»/);
  });

  it("una tabla de solo lectura no se edita", () => {
    expect(planEdit(shape({ readOnly: "Es una vista." }), { oid: 1, columns: ["id"] })).toBe("Es una vista.");
  });

  it("una clave compuesta pide todas sus columnas", () => {
    const compuesta = shape({ key: { name: "k", kind: "primary", columns: ["id", "estado"] } });
    expect(planEdit(compuesta, { oid: 1, columns: ["id", "nombre"] })).toMatch(/«estado»/);
  });
});

describe("buildChanges", () => {
  const plan = planEdit(shape(), { oid: 1, columns: ["id", "nombre", "estado"] });
  if (typeof plan === "string") throw new Error(plan);
  const rows = [
    ["1", "Ana", "activo"],
    ["2", "Luis", null],
  ];

  it("lleva la clave, los originales y los nuevos solo de lo que cambió", () => {
    const edits = withEdit(new Map(), rows, 0, 1, "Ana María");
    expect(buildChanges(plan, rows, edits)).toEqual([
      { kind: "update", key: ["1"], original: { nombre: "Ana" }, changes: { nombre: "Ana María" } },
    ]);
  });

  it("volver al valor original quita el cambio", () => {
    let edits = withEdit(new Map(), rows, 0, 1, "X");
    edits = withEdit(edits, rows, 0, 1, "Ana");
    expect(editedRows(edits)).toBe(0);
    expect(buildChanges(plan, rows, edits)).toEqual([]);
  });

  it("un NULL nuevo y un NULL original se distinguen de la cadena vacía", () => {
    let edits = withEdit(new Map(), rows, 1, 2, "");
    expect(buildChanges(plan, rows, edits)[0]).toMatchObject({ original: { estado: null }, changes: { estado: "" } });
    edits = withEdit(new Map(), rows, 0, 2, null);
    expect(buildChanges(plan, rows, edits)[0]).toMatchObject({ original: { estado: "activo" }, changes: { estado: null } });
  });

  it("no toca columnas no editables aunque lleguen en las ediciones", () => {
    const edits = withEdit(new Map(), rows, 0, 0, "99");
    expect(buildChanges(plan, rows, edits)).toEqual([]);
  });
});
