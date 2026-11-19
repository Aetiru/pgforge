/**
 * Valores posibles al comparar una columna: `estado = '|` ofrece las etiquetas del enum o los
 * valores más frecuentes según las estadísticas del servidor.
 *
 * Qué columna es y si el cursor está dentro de una comilla lo decide `valueContextAt` (puro). Los
 * valores vienen de `column_values`, que **no recorre la tabla**: sin `ANALYZE` no hay nada que
 * ofrecer y la fuente calla, que es mejor que sugerir basura.
 */

import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import type { ColumnValues, SchemaRelation } from "../ipc";
import { valueContextAt } from "../sql-values-context";
import { TtlCache } from "./ttl-cache";

/** Un minuto: lo frecuente de una columna cambia despacio y cada tecla no puede costar una consulta. */
const TTL_MS = 60_000;

export type ValueKind = "number" | "boolean" | "text";

/**
 * Cómo se escribe un valor de este tipo: un número o un booleano van sin comillas, y todo lo demás
 * (texto, fechas, uuid, enums) entre comillas simples.
 */
export function valueKind(typeName: string | undefined): ValueKind {
  if (!typeName) return "text";
  const type = typeName.toLowerCase().replace(/\(.*\)/, "").trim();
  if (type === "boolean" || type === "bool") return "boolean";
  if (
    /^(smallint|integer|bigint|int|int2|int4|int8|numeric|decimal|real|double precision|float4|float8|oid|smallserial|serial|bigserial|money)$/.test(
      type,
    )
  ) {
    return "number";
  }
  return "text";
}

/** Escribe un valor como literal de SQL: las comillas simples se duplican. */
export const quoteLiteral = (value: string) => `'${value.replace(/'/g, "''")}'`;

/**
 * Lo que se inserta al elegir un valor.
 *
 * - Dentro de una comilla ya abierta: el valor escapado, y la comilla de cierre solo si nadie la
 *   puso (el cierre automático de paréntesis y comillas suele haberla puesto ya, y duplicarla deja
 *   `''`).
 * - Sin comilla: el literal entero, salvo en números y booleanos, que van tal cual. Las estadísticas
 *   guardan los booleanos como `t`/`f`, que no es SQL.
 */
export function valueInsertText(
  value: string,
  options: { insideQuote: boolean; kind: ValueKind; quoteFollows: boolean },
): string {
  if (options.insideQuote) {
    const escaped = value.replace(/'/g, "''");
    return options.quoteFollows ? escaped : `${escaped}'`;
  }
  if (options.kind === "boolean") return value === "t" ? "true" : value === "f" ? "false" : value;
  if (options.kind === "number") return value;
  return quoteLiteral(value);
}

/** La frecuencia como porcentaje para la lista; `null` (una etiqueta de enum) no lleva ninguna. */
export function frequencyLabel(frequency: number | null): string | undefined {
  if (frequency === null) return undefined;
  const percent = frequency * 100;
  return percent < 1 ? "< 1 %" : `${Math.round(percent)} %`;
}

/** Los candidatos como opciones de la lista, sin tocar el documento todavía. */
export function valueOptions(
  values: ColumnValues,
  kind: ValueKind,
  insideQuote: boolean,
  quoteFollows: boolean,
): Completion[] {
  if (values.kind === "none") return [];
  return values.values.map((item, index) => ({
    label: item.value,
    detail: frequencyLabel(item.frequency),
    type: values.kind === "enum" ? "enum" : "constant",
    // Los más frecuentes arriba, que es el orden en que llegan.
    boost: -index,
    apply: valueInsertText(item.value, { insideQuote, kind, quoteFollows }),
  }));
}

export interface ValuesDeps {
  relations: () => SchemaRelation[];
  fetch: (schema: string, table: string, column: string) => Promise<ColumnValues>;
  enabled?: () => boolean;
}

export function valuesCompletion(deps: ValuesDeps) {
  const cache = new TtlCache<ColumnValues>(TTL_MS);

  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    if (deps.enabled && !deps.enabled()) return null;

    const text = context.state.doc.toString();
    const relations = deps.relations();
    const found = valueContextAt(text, context.pos, relations);
    if (!found) return null;

    // `FROM clientes` sin esquema: la de `public` si existe, como resuelve el servidor de fábrica.
    const relation =
      relations.find((r) => r.name === found.table && (found.schema === null ? r.schema === "public" : r.schema === found.schema)) ??
      relations.find((r) => r.name === found.table && (found.schema === null || r.schema === found.schema));
    if (!relation) return null;

    const kind = valueKind(relation.columns.find((c) => c.name === found.column)?.typeName);

    let values: ColumnValues;
    try {
      values = await cache.get(`${relation.schema}.${relation.name}.${found.column}`, () =>
        deps.fetch(relation.schema, relation.name, found.column),
      );
    } catch {
      // Sin conexión o sin permiso: la lista de siempre, sin cartel.
      return null;
    }
    // El usuario siguió escribiendo mientras llegaba la respuesta: la lista ya no es de este lugar.
    if (context.aborted) return null;

    const quoteFollows = context.state.sliceDoc(context.pos, context.pos + 1) === "'";
    const options = valueOptions(values, kind, found.insideQuote, quoteFollows);
    if (options.length === 0) return null;

    return {
      from: found.from,
      options,
      // Mientras lo escrito no cierre la comilla, la lista sigue valiendo sin volver a pedirla.
      validFor: /^[^']*$/,
    };
  };
}
