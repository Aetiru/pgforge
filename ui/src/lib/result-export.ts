/**
 * Exportar a JSON y Markdown lo que está cargado en pantalla.
 *
 * A diferencia de CSV/TSV/binario (que vuelven al servidor con un `COPY`, así el archivo trae todas
 * las filas), estos dos se arman acá con lo que ya llegó: `COPY` no tiene formato JSON ni Markdown,
 * y envolver la consulta en `json_agg` pasaría todo el resultado por un solo valor de texto (con
 * los escapes de `COPY`) y por la memoria del servidor. El costo es que respetan el techo de filas;
 * el diálogo lo avisa cuando el resultado vino cortado.
 *
 * Funciones puras: el texto que se escribe es el que se prueba.
 */

import type { ResultData } from "./result-diff";

const INTEGER = new Set(["int2", "int4", "smallint", "integer", "oid"]);
const FLOAT = new Set(["float4", "float8", "real", "double precision"]);
const BOOL = new Set(["bool", "boolean"]);
const JSON_TYPES = new Set(["json", "jsonb"]);

/**
 * El valor de una celda como JSON. Con el tipo conocido, números, booleanos y json se escriben como
 * lo que son; sin él (o con `int8`/`numeric`, que pierden precisión como número de JavaScript) se
 * queda el texto, que es lo que el servidor mandó.
 */
export function jsonValue(value: string | null, type: string | null | undefined): unknown {
  if (value === null) return null;
  const t = (type ?? "").trim().toLowerCase();
  if (INTEGER.has(t) || FLOAT.has(t)) {
    const n = Number(value);
    return Number.isFinite(n) && value.trim() !== "" ? n : value;
  }
  if (BOOL.has(t)) {
    const v = value.trim().toLowerCase();
    if (v === "t" || v === "true") return true;
    if (v === "f" || v === "false") return false;
    return value;
  }
  if (JSON_TYPES.has(t)) {
    try {
      return JSON.parse(value);
    } catch {
      return value;
    }
  }
  return value;
}

/** Nombres únicos: `SELECT a, a` daría un objeto con una sola clave y perdería una columna. */
export function uniqueNames(columns: string[]): string[] {
  const seen = new Map<string, number>();
  return columns.map((name) => {
    const n = (seen.get(name) ?? 0) + 1;
    seen.set(name, n);
    return n === 1 ? name : `${name}_${n}`;
  });
}

export function toJson(data: ResultData, types: string[] | null = null): string {
  const names = uniqueNames(data.columns);
  const objects = data.rows.map((row) => {
    const out: Record<string, unknown> = {};
    names.forEach((name, i) => {
      out[name] = jsonValue(row[i] ?? null, types?.[i]);
    });
    return out;
  });
  return `${JSON.stringify(objects, null, 2)}\n`;
}

function cell(value: string | null): string {
  if (value === null) return "NULL";
  return value.replace(/\\/g, "\\\\").replace(/\|/g, "\\|").replace(/\r?\n/g, "<br>");
}

export function toMarkdown(data: ResultData): string {
  const header = data.columns.map(cell);
  const lines = [
    `| ${header.join(" | ")} |`,
    `| ${header.map(() => "---").join(" | ")} |`,
    ...data.rows.map((row) => `| ${data.columns.map((_, i) => cell(row[i] ?? null)).join(" | ")} |`),
  ];
  return `${lines.join("\n")}\n`;
}
