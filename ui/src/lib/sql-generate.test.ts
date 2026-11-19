import { describe, expect, it } from "vitest";
import type { TableShape } from "./ipc";
import { generateSql, sqlIdent } from "./sql-generate";

const col = (name: string, typeName: string, generated = false) => ({
  name,
  typeName,
  notNull: false,
  default: null,
  generated,
  comment: null,
});

const shape: TableShape = {
  oid: 1,
  schema: "public",
  name: "pedidos",
  columns: [col("id", "bigint", true), col("cliente_id", "bigint"), col("Nota", "text"), col("order", "integer")],
  key: { name: "pedidos_pkey", kind: "primary", columns: ["id"] },
  readOnly: null,
  foreignKeys: [],
  referencedBy: [],
};

describe("generateSql", () => {
  it("SELECT lista todas las columnas, citando lo que hace falta", () => {
    expect(generateSql(shape, "select")).toBe(
      'SELECT id,\n       cliente_id,\n       "Nota",\n       "order"\nFROM public.pedidos\nLIMIT 100;\n',
    );
  });

  it("INSERT omite lo que calcula el servidor y comilla solo el texto", () => {
    expect(generateSql(shape, "insert")).toBe(
      `INSERT INTO public.pedidos (cliente_id, "Nota", "order")\nVALUES ({{cliente_id}}, '{{Nota}}', {{order}});\n`,
    );
  });

  it("UPDATE no toca la clave y la usa en el WHERE", () => {
    const sql = generateSql(shape, "update");
    expect(sql).toContain("SET cliente_id = {{cliente_id}}");
    expect(sql).toContain("WHERE id = {{id}};");
    expect(sql).not.toContain("SET id");
  });

  it("DELETE usa la clave", () => {
    expect(generateSql(shape, "delete")).toBe("DELETE FROM public.pedidos\nWHERE id = {{id}};\n");
  });

  it("sin clave no deja un UPDATE o DELETE que toque todo", () => {
    const sinClave = { ...shape, key: null };
    expect(generateSql(sinClave, "delete")).toContain("WHERE false");
    expect(generateSql(sinClave, "update")).toContain("WHERE false");
  });

  it("los nombres con espacios dan una variable válida", () => {
    const raro = { ...shape, columns: [col("mi col", "text")], key: null };
    expect(generateSql(raro, "insert")).toContain("'{{mi_col}}'");
  });

  it("cita palabras reservadas", () => {
    expect(sqlIdent("user")).toBe('"user"');
    expect(sqlIdent("nombre")).toBe("nombre");
  });
});
