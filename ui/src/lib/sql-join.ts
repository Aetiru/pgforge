/**
 * El `ON` de un `JOIN`, propuesto a partir de las claves foráneas.
 *
 * Escrito `JOIN clientes c `, el autocompletado ofrece `ON c.id = p.cliente_id` para cada clave
 * foránea que une la tabla nueva con alguna de las que ya están en la sentencia, en cualquiera de
 * los dos sentidos. Lee el texto y no el árbol de sintaxis, por lo mismo que `sql-complete.ts`: el
 * árbol de un SQL a medio escribir tiene nodos de error justo donde está el cursor.
 *
 * Una clave compuesta se ofrece entera, con `AND`. Es puro y con Vitest.
 */

import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import type { ForeignKeyRef } from "./ipc";
import { tablesInScope, type TableRef } from "./sql-complete";

const NAME = String.raw`(?:"[^"]*"|[A-Za-z_À-￿][\w$À-￿]*)`;

/**
 * El final de lo escrito: `JOIN [esquema.]tabla [AS] [alias] [ON] [lo que se tipea]`.
 * Los grupos son esquema, tabla, alias, `ON` y la palabra a medio escribir.
 */
const TAIL = new RegExp(
  String.raw`\bjoin\s+(?:LATERAL\s+)?(?:(${NAME})\s*\.\s*)?(${NAME})(?:\s+(?:AS\s+)?(${NAME}))?\s+(?:(ON)\s+)?([\w$]*)$`,
  "i",
);

/** Palabras que, en el lugar del alias, son sintaxis del `JOIN` y no un alias. */
const NOT_ALIAS = new Set(["on", "using", "where", "left", "right", "inner", "full", "cross", "natural", "join", "group", "order", "limit"]);

export interface JoinOption {
  label: string;
  detail: string;
}

function unquote(identifier: string): string {
  return identifier.startsWith('"') ? identifier.slice(1, -1) : identifier;
}

/** El identificador tal cual se escribe: con comillas solo si hace falta. */
export function ident(name: string): string {
  return /^[a-z_][a-z0-9_$]*$/.test(name) ? name : `"${name.replace(/"/g, '""')}"`;
}

/** Cómo se llama la tabla en la sentencia: el alias si lo tiene, si no su nombre. */
function handle(ref: TableRef): string {
  return ident(ref.alias ?? ref.name);
}

function matches(ref: TableRef, schema: string, table: string): boolean {
  return ref.name === table && (ref.schema === null || ref.schema === schema);
}

export interface JoinContext {
  joined: TableRef;
  /** `true` si ya escribió `ON`: entonces se ofrece solo la condición. */
  hasOn: boolean;
  /** Lo que lleva escrito de la palabra en curso. */
  partial: string;
}

/** Dónde está el cursor respecto de un `JOIN`, o `null` si no es un lugar donde ofrecer un `ON`. */
export function joinContext(before: string): JoinContext | null {
  const match = TAIL.exec(before);
  if (!match) return null;

  const [, schema, table, alias, on, partial] = match;
  if (alias && NOT_ALIAS.has(alias.toLowerCase())) {
    // `JOIN t ON x`: el «alias» capturado era el `ON`, que entonces no se tomó como tal.
    if (alias.toLowerCase() !== "on") return null;
    return {
      joined: { schema: schema ? unquote(schema) : null, name: unquote(table), alias: null },
      hasOn: true,
      partial,
    };
  }

  // La palabra en curso es el `ON` a medio escribir (`o`, `on`): es lo que se completa.
  return {
    joined: {
      schema: schema ? unquote(schema) : null,
      name: unquote(table),
      alias: alias ? unquote(alias) : null,
    },
    hasOn: on !== undefined,
    partial,
  };
}

/**
 * Las condiciones que proponen las claves foráneas entre la tabla nueva y las demás de la sentencia.
 * `others` son las tablas que ya estaban, sin la nueva.
 */
export function joinConditions(
  joined: TableRef,
  others: readonly TableRef[],
  foreignKeys: readonly ForeignKeyRef[],
  resolveSchema: (ref: TableRef) => string | null,
): JoinOption[] {
  const options: JoinOption[] = [];
  const joinedSchema = resolveSchema(joined);
  if (joinedSchema === null) return options;

  const joinedHandle = handle(joined);

  for (const other of others) {
    const otherSchema = resolveSchema(other);
    if (otherSchema === null) continue;
    const otherHandle = handle(other);

    for (const fk of foreignKeys) {
      let pairs: string[] | null = null;

      if (matches(joined, fk.schema, fk.table) && fk.schema === joinedSchema &&
          matches(other, fk.refSchema, fk.refTable) && fk.refSchema === otherSchema) {
        // La nueva es la que referencia: `nueva.col = otra.clave`.
        pairs = fk.columns.map((c, i) => `${joinedHandle}.${ident(c)} = ${otherHandle}.${ident(fk.refColumns[i])}`);
      } else if (matches(other, fk.schema, fk.table) && fk.schema === otherSchema &&
          matches(joined, fk.refSchema, fk.refTable) && fk.refSchema === joinedSchema) {
        // La nueva es la referida: `otra.col = nueva.clave`.
        pairs = fk.columns.map((c, i) => `${otherHandle}.${ident(c)} = ${joinedHandle}.${ident(fk.refColumns[i])}`);
      }

      if (pairs && pairs.length > 0) {
        const label = pairs.join(" AND ");
        if (!options.some((option) => option.label === label)) {
          options.push({ label, detail: `clave foránea ${other.alias ?? other.name}` });
        }
      }
    }
  }
  return options;
}

/** La fuente de autocompletado del `ON`, para sumarla a la del dialecto. */
export function joinCompletion(
  foreignKeys: () => readonly ForeignKeyRef[],
  schemaOf: (ref: TableRef) => string | null,
) {
  return (context: CompletionContext): CompletionResult | null => {
    const fks = foreignKeys();
    if (fks.length === 0) return null;

    const line = context.state.doc.sliceString(0, context.pos);
    // Solo la sentencia del cursor: un `;` antes corta lo que hubiera de un `JOIN` anterior.
    const before = line.slice(line.lastIndexOf(";") + 1);
    const ctx = joinContext(before);
    if (!ctx) return null;
    // Sin `ON` escrito, la palabra en curso solo es un `ON` a medias si es prefijo de «on»: si no,
    // es el alias que se está tecleando y reemplazarlo por una condición estorbaría.
    if (!ctx.hasOn && !context.explicit && !"on".startsWith(ctx.partial.toLowerCase())) return null;

    const refs = tablesInScope(context.state.doc.toString(), context.pos);
    const joinedKey = (ref: TableRef) => ref.name === ctx.joined.name && ref.alias === ctx.joined.alias;
    const others = refs.filter((ref) => !joinedKey(ref));
    const joined = refs.find(joinedKey) ?? ctx.joined;

    const conditions = joinConditions(joined, others, fks, schemaOf);
    if (conditions.length === 0) return null;

    return {
      from: context.pos - ctx.partial.length,
      options: conditions.map((condition) => ({
        label: ctx.hasOn ? condition.label : `ON ${condition.label}`,
        detail: condition.detail,
        type: "keyword",
        boost: 99,
      })),
      validFor: /^[\w$]*$/,
    };
  };
}
