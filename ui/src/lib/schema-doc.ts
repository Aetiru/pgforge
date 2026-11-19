/**
 * La documentación de un esquema en Markdown.
 *
 * Sale del mismo `SchemaGraph` que dibuja el diagrama —tablas, columnas con sus comentarios y claves
 * foráneas— más las notas locales, que son lo único que no vive en la base. Se puede volver a
 * generar cuando el modelo cambia, que es el punto: una documentación escrita a mano se desactualiza
 * el día que alguien agrega una columna. Es puro y con Vitest.
 */
import type { GraphEdge, GraphTable, SchemaGraph } from "./ipc";
import type { ObjectNote } from "./notes";

export interface DocOptions {
  /** La nota local de una tabla, si hay. */
  noteOf?: (table: GraphTable) => ObjectNote | null;
  /** Incluir el diagrama de relaciones (Mermaid). */
  diagram: boolean;
  /** Fecha que se muestra: se inyecta para que la salida sea reproducible en las pruebas. */
  date: string;
}

const ACTION: Record<string, string> = {
  cascade: "CASCADE",
  setNull: "SET NULL",
  setDefault: "SET DEFAULT",
  restrict: "RESTRICT",
  noAction: "NO ACTION",
};

/** Una celda de tabla Markdown: sin `|` ni saltos de línea, que la romperían. */
function cell(text: string | null | undefined): string {
  return (text ?? "").replace(/\|/g, "\\|").replace(/\s*\n\s*/g, " ").trim();
}

function code(text: string): string {
  return `\`${text.replace(/`/g, "'")}\``;
}

/** Un identificador de Mermaid: letras, números y guion bajo. */
function mermaidId(name: string): string {
  const clean = name.replace(/[^\p{L}\p{N}_]/gu, "_");
  return /^\d/.test(clean) ? `_${clean}` : clean;
}

/** El destino de una arista, como se lee: `tabla.columna` o `esquema.tabla`. */
function targetOf(edge: GraphEdge, graph: SchemaGraph): { table: string; columns: string } {
  const table =
    edge.targetLabel ?? graph.tables.find((candidate) => candidate.oid === edge.target)?.name ?? "?";
  return { table, columns: edge.targetColumns.join(", ") };
}

function section(table: GraphTable, graph: SchemaGraph, options: DocOptions): string[] {
  const lines: string[] = [`## ${table.name}`, ""];
  if (table.comment) lines.push(`> ${cell(table.comment)}`, "");

  const note = options.noteOf?.(table) ?? null;
  if (note && note.note.trim() !== "") lines.push(`**Nota:** ${cell(note.note)}`, "");
  if (note && note.tags.length > 0) lines.push(`**Etiquetas:** ${note.tags.map((tag) => `#${tag}`).join(" ")}`, "");

  lines.push("| Columna | Tipo | Nulo | Clave | Descripción |", "| --- | --- | --- | --- | --- |");
  for (const column of table.columns) {
    const key = [column.primaryKey ? "PK" : "", column.foreignKey ? "FK" : ""].filter(Boolean).join(", ");
    lines.push(
      `| ${code(column.name)} | ${cell(column.typeName)} | ${column.notNull ? "no" : "sí"} | ${key} | ${cell(column.comment)} |`,
    );
  }
  lines.push("");

  const outgoing = graph.edges.filter((edge) => edge.source === table.oid);
  if (outgoing.length > 0) {
    lines.push("**Referencia a:**", "");
    for (const edge of outgoing) {
      const target = targetOf(edge, graph);
      lines.push(
        `- ${edge.sourceColumns.map(code).join(", ")} → ${code(target.table)}(${target.columns}) · ` +
          `al borrar: ${ACTION[edge.onDelete] ?? edge.onDelete}, al cambiar: ${ACTION[edge.onUpdate] ?? edge.onUpdate}`,
      );
    }
    lines.push("");
  }

  const incoming = graph.edges.filter((edge) => edge.target === table.oid);
  if (incoming.length > 0) {
    lines.push("**Referenciada por:**", "");
    for (const edge of incoming) {
      const source = graph.tables.find((candidate) => candidate.oid === edge.source)?.name ?? "?";
      lines.push(`- ${code(source)}(${edge.sourceColumns.join(", ")})`);
    }
    lines.push("");
  }
  return lines;
}

/** El diagrama de entidad-relación en Mermaid, que GitHub y GitLab dibujan solos. */
export function mermaidDiagram(graph: SchemaGraph): string[] {
  const lines = ["```mermaid", "erDiagram"];
  for (const table of graph.tables) {
    lines.push(`  ${mermaidId(table.name)} {`);
    for (const column of table.columns) {
      const key = column.primaryKey ? " PK" : column.foreignKey ? " FK" : "";
      lines.push(`    ${mermaidId(column.typeName) || "tipo"} ${mermaidId(column.name)}${key}`);
    }
    lines.push("  }");
  }
  for (const edge of graph.edges) {
    const source = graph.tables.find((table) => table.oid === edge.source);
    const target = graph.tables.find((table) => table.oid === edge.target);
    // Una tabla de otro esquema no está dibujada: la arista no tiene a dónde apuntar.
    if (!source || !target) continue;
    lines.push(
      `  ${mermaidId(target.name)} ||--o{ ${mermaidId(source.name)} : "${edge.sourceColumns.join(", ").replace(/"/g, "'")}"`,
    );
  }
  lines.push("```");
  return lines;
}

export function schemaMarkdown(graph: SchemaGraph, options: DocOptions): string {
  const lines: string[] = [
    `# Esquema ${code(graph.schema)} de ${code(graph.database)}`,
    "",
    `Generado con pgForge el ${options.date}. ${graph.tables.length} ${graph.tables.length === 1 ? "tabla" : "tablas"}.`,
    "",
  ];

  if (graph.tables.length > 0) {
    lines.push("## Índice", "");
    for (const table of graph.tables) lines.push(`- ${table.name}${table.comment ? ` — ${cell(table.comment)}` : ""}`);
    lines.push("");
  }

  if (options.diagram && graph.edges.some((edge) => edge.targetLabel === undefined || edge.targetLabel === null)) {
    lines.push("## Diagrama de relaciones", "", ...mermaidDiagram(graph), "");
  }

  for (const table of graph.tables) lines.push(...section(table, graph, options));
  return `${lines.join("\n").trimEnd()}\n`;
}
