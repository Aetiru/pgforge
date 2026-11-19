import { describe, expect, it } from "vitest";
import type { GraphTable, SchemaGraph } from "./ipc";
import { mermaidDiagram, schemaMarkdown } from "./schema-doc";

const col = (position: number, name: string, typeName: string, over = {}) => ({
  position,
  name,
  typeName,
  notNull: false,
  primaryKey: false,
  foreignKey: false,
  ...over,
});

const clientes: GraphTable = {
  oid: 1,
  name: "clientes",
  kind: "table",
  comment: "Quien compra",
  columns: [col(1, "id", "bigint", { primaryKey: true, notNull: true }), col(2, "nombre", "character varying(80)", { comment: "Nombre | legal" })],
} as GraphTable;

const pedidos: GraphTable = {
  oid: 2,
  name: "pedidos",
  kind: "table",
  comment: null,
  columns: [col(1, "id", "bigint", { primaryKey: true }), col(2, "cliente_id", "bigint", { foreignKey: true, notNull: true })],
} as unknown as GraphTable;

const graph: SchemaGraph = {
  database: "ventas",
  schema: "public",
  tables: [clientes, pedidos],
  edges: [
    {
      name: "pedidos_cliente_fk",
      source: 2,
      target: 1,
      sourceColumns: ["cliente_id"],
      targetColumns: ["id"],
      onUpdate: "noAction",
      onDelete: "cascade",
    },
  ],
};

describe("schemaMarkdown", () => {
  const md = schemaMarkdown(graph, { diagram: true, date: "2026-11-18", noteOf: (t) => (t.name === "clientes" ? { note: "Se carga del CRM", tags: ["crm"] } : null) });

  it("lleva título, índice y una sección por tabla", () => {
    expect(md).toContain("# Esquema `public` de `ventas`");
    expect(md).toContain("2 tablas");
    expect(md).toContain("- clientes — Quien compra");
    expect(md).toContain("## pedidos");
  });

  it("la tabla de columnas escapa las barras del comentario", () => {
    expect(md).toContain("| `nombre` | character varying(80) | sí |  | Nombre \\| legal |");
    expect(md).toContain("| `id` | bigint | no | PK |  |");
  });

  it("dice a quién referencia y quién la referencia, con las acciones", () => {
    expect(md).toContain("- `cliente_id` → `clientes`(id) · al borrar: CASCADE, al cambiar: NO ACTION");
    expect(md).toContain("**Referenciada por:**\n\n- `pedidos`(cliente_id)");
  });

  it("incluye las notas locales cuando se las pasa", () => {
    expect(md).toContain("**Nota:** Se carga del CRM");
    expect(md).toContain("**Etiquetas:** #crm");
  });

  it("el diagrama es un bloque mermaid con la relación en el sentido correcto", () => {
    expect(md).toContain("```mermaid\nerDiagram");
    expect(md).toContain('clientes ||--o{ pedidos : "cliente_id"');
  });

  it("sin diagrama no lo incluye", () => {
    expect(schemaMarkdown(graph, { diagram: false, date: "x" })).not.toContain("mermaid");
  });
});

describe("mermaidDiagram", () => {
  it("sanea nombres y tipos con espacios o paréntesis", () => {
    const lines = mermaidDiagram(graph).join("\n");
    expect(lines).toContain("character_varying_80_ nombre");
  });
});
