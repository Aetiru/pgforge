/**
 * Columnas de las tablas del `FROM`, para completar sin calificar.
 *
 * `@codemirror/lang-sql` solo ofrece columnas detrás de `tabla.` o de un alias: escrito el `FROM`,
 * un `SELECT ` sin calificar no completa nada, que es como se escribe la mayoría de las consultas.
 * Esto lo agrega **encima** de la fuente del dialecto, sin reemplazarla: las palabras clave, los
 * esquemas y `tabla.` siguen saliendo de ella.
 *
 * De qué tablas se trata se saca leyendo el texto, no el árbol de sintaxis: el árbol de un SQL a
 * medio escribir —que es cuando se completa— tiene nodos de error justo donde está el cursor.
 * Delimitar la sentencia por el `;` más cercano es una aproximación a propósito, y no una copia en
 * TypeScript del partidor del núcleo: acá una equivocación cuesta una sugerencia de más en la lista,
 * no una consulta mal mandada, y tener dos partidores de verdad es lo que se desincroniza.
 */

import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import type { RelationColumn, SchemaRelation } from "./ipc";
import { lex, type Token } from "./statement-risk";

/** Una tabla nombrada en el `FROM`/`JOIN`, con el alias que le hayan puesto. */
export interface TableRef {
  /** `null` cuando se escribió sin calificar (`FROM clientes`). */
  schema: string | null;
  name: string;
  alias: string | null;
}

/** Palabras que cierran la lista de tablas: lo que viene después ya no nombra una tabla. */
const ENDS_FROM = new Set([
  "where",
  "group",
  "order",
  "having",
  "limit",
  "offset",
  "union",
  "except",
  "intersect",
  "returning",
  "window",
  "fetch",
  "for",
  "on",
  "using",
  "set",
  "values",
  "join",
  "inner",
  "left",
  "right",
  "full",
  "cross",
  "natural",
  "lateral",
  // `as` no entra: es lo que **introduce** el alias, no lo que cierra la lista.
  "with",
  "select",
]);

/** Saca comentarios sin mover ninguna posición: se reemplazan por espacios, no se borran. */
function blankComments(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, (match) => " ".repeat(match.length))
    .replace(/\/\*[\s\S]*?\*\//g, (match) => match.replace(/[^\n]/g, " "));
}

/** La sentencia donde cae el cursor, delimitada por el `;` más cercano de cada lado. */
function statementAround(sql: string, cursor: number): string {
  const before = sql.lastIndexOf(";", Math.max(0, cursor - 1));
  const after = sql.indexOf(";", cursor);
  return sql.slice(before + 1, after === -1 ? sql.length : after);
}

function unquote(identifier: string): string {
  return identifier.startsWith('"') ? identifier.slice(1, -1) : identifier;
}

/** `esquema.tabla` o `tabla`, con o sin comillas. */
const NAME = String.raw`(?:"[^"]*"|[A-Za-z_À-￿][\w$À-￿]*)`;
const REFERENCE = new RegExp(
  String.raw`^(${NAME})(?:\s*\.\s*(${NAME}))?(?:\s+(?:AS\s+)?(${NAME}))?`,
  "i",
);

function parseReference(item: string): TableRef | null {
  const text = item.trim();
  // Una subconsulta o una función devuelven columnas que el catálogo no conoce.
  if (text === "" || text.startsWith("(")) return null;

  const match = REFERENCE.exec(text);
  if (!match) return null;

  const [, first, second, alias] = match;
  const schema = second ? unquote(first) : null;
  const name = unquote(second ?? first);
  if (alias && ENDS_FROM.has(alias.toLowerCase())) {
    return { schema, name, alias: null };
  }
  return { schema, name, alias: alias ? unquote(alias) : null };
}

/** Corta la lista de tablas en las comas de primer nivel, dejando las de adentro de paréntesis. */
function topLevelItems(clause: string): string[] {
  const items: string[] = [];
  let depth = 0;
  let start = 0;

  for (let at = 0; at < clause.length; at++) {
    const character = clause[at];
    if (character === "(") depth += 1;
    else if (character === ")") depth -= 1;
    else if (character === "," && depth === 0) {
      items.push(clause.slice(start, at));
      start = at + 1;
    }
  }
  items.push(clause.slice(start));
  return items;
}

/**
 * Las tablas que la sentencia del cursor tiene a la vista.
 *
 * Cubre `FROM` (con su lista separada por comas), cada `JOIN`, y el objeto de un `UPDATE` o de un
 * `INSERT INTO`, que son los tres lugares donde uno escribe columnas sin calificar.
 */
export function tablesInScope(sql: string, cursor: number): TableRef[] {
  const statement = blankComments(statementAround(sql, cursor));
  const found: TableRef[] = [];

  const clauses = /\b(from|join|update|into)\b/gi;
  let clause: RegExpExecArray | null;

  while ((clause = clauses.exec(statement)) !== null) {
    const rest = statement.slice(clause.index + clause[0].length);
    // Solo el `FROM` lleva lista; un `JOIN` nombra una tabla y sigue con `ON`.
    const items =
      clause[1].toLowerCase() === "from" ? topLevelItems(cutAtKeyword(rest)) : [cutAtKeyword(rest)];

    for (const item of items) {
      const reference = parseReference(item);
      if (reference && !found.some((seen) => same(seen, reference))) found.push(reference);
    }
  }

  return found;
}

function same(a: TableRef, b: TableRef): boolean {
  return a.schema === b.schema && a.name === b.name && a.alias === b.alias;
}

/** Recorta hasta la primera palabra que ya no puede ser parte de un nombre de tabla ni su alias. */
function cutAtKeyword(rest: string): string {
  const words = /[A-Za-z_][\w$]*/g;
  let word: RegExpExecArray | null;
  let first = true;

  while ((word = words.exec(rest)) !== null) {
    // La primera palabra es el nombre de la tabla, aunque se llame como una palabra clave.
    if (!first && ENDS_FROM.has(word[0].toLowerCase())) return rest.slice(0, word.index);
    first = false;
  }
  return rest;
}

/** Una columna ofrecida, con la tabla de la que sale. */
export interface ColumnOption {
  label: string;
  /** Qué escribir al lado en la lista: el alias si lo hay, y si no el nombre de la tabla. */
  detail: string;
}

/**
 * Las columnas de las tablas a la vista, sin repetir.
 *
 * Una columna que está en dos tablas se ofrece una vez por cada una: cuál de las dos se quiere es
 * justo lo que el `detail` contesta, y ofrecerla una sola vez escondería la ambigüedad que después
 * el servidor rechaza.
 */
export function columnOptions(relations: SchemaRelation[], refs: TableRef[]): ColumnOption[] {
  const options: ColumnOption[] = [];

  for (const ref of refs) {
    const relation = relations.find(
      (candidate) =>
        candidate.name === ref.name && (ref.schema === null || candidate.schema === ref.schema),
    );
    if (!relation) continue;

    const detail = ref.alias ?? relation.name;
    for (const column of relation.columns) {
      if (!options.some((option) => option.label === column.name && option.detail === detail)) {
        options.push({ label: column.name, detail });
      }
    }
  }

  return options;
}

// ---------------------------------------------------------------------------------------------
// Dónde está el cursor: cláusula, CTE, alias de la lista SELECT y objetivo de un INSERT
// ---------------------------------------------------------------------------------------------

/** En qué parte de la sentencia está el cursor. Decide qué columnas tiene sentido ofrecer. */
export type Clause =
  | "select"
  | "from"
  | "where"
  | "group"
  | "order"
  | "having"
  | "set"
  | "values"
  | "insert-columns"
  | "on-conflict"
  | "other";

interface Frame {
  clause: Clause;
  /** Índice del `(` que abrió este nivel; `-1` para el nivel de la sentencia. */
  open: number;
  /** Índice del último `SELECT` de este nivel. */
  select: number | null;
  /** Ya pasó el `ON` de un `JOIN`: una coma que sigue no separa tablas. */
  afterOn: boolean;
}

interface Analysis {
  /** Los tokens de la sentencia del cursor, con posiciones absolutas. */
  tokens: Token[];
  /** Cuántos tokens quedan antes del cursor, sin contar la palabra que se está tecleando. */
  upto: number;
  clause: Clause;
  frame: Frame;
  /** Se espera el nombre de una tabla: justo después de `FROM`, `JOIN`, `UPDATE` o una coma del `FROM`. */
  tableSlot: boolean;
  /** La tabla de `INSERT INTO t`, si la sentencia es un `INSERT`. */
  insertTarget: { schema: string | null; name: string } | null;
}

const word = (token: Token | undefined) => (token?.type === "word" ? token.value.toUpperCase() : "");
const isPunct = (token: Token | undefined, value: string) =>
  token?.type === "punct" && token.value === value;

/** El nombre que escribe un identificador, con o sin comillas; `null` si no lo es. */
function identName(token: Token | undefined): string | null {
  if (token?.type === "word" && !/^[0-9]/.test(token.value)) return token.value;
  if (token?.type === "opaque" && token.value.startsWith('"')) return token.inner;
  return null;
}

function insertTargetOf(tokens: Token[]): Analysis["insertTarget"] {
  for (let i = 0; i < tokens.length - 1; i += 1) {
    if (word(tokens[i]) !== "INSERT" || word(tokens[i + 1]) !== "INTO") continue;
    const first = identName(tokens[i + 2]);
    if (first === null) return null;
    if (isPunct(tokens[i + 3], ".") && identName(tokens[i + 4]) !== null) {
      return { schema: first, name: identName(tokens[i + 4]) as string };
    }
    return { schema: null, name: first };
  }
  return null;
}

function analyze(sql: string, cursor: number): Analysis {
  const all = lex(sql).tokens;

  // La sentencia es lo que queda entre dos `;` de primer nivel (los de una cadena no cuentan).
  let from = 0;
  let to = all.length;
  let startPos = 0;
  for (let i = 0; i < all.length; i += 1) {
    if (!isPunct(all[i], ";")) continue;
    if (all[i].start >= cursor) {
      to = i;
      break;
    }
    from = i + 1;
    startPos = all[i].end;
  }
  void startPos;
  const tokens = all.slice(from, to);

  // La palabra que se está tecleando no es contexto todavía: `ORDER BY ve|`.
  let upto = tokens.filter((token) => token.end <= cursor).length;
  const last = tokens[upto - 1];
  if (last?.type === "word" && last.end === cursor) upto -= 1;

  const frames: Frame[] = [{ clause: "other", open: -1, select: null, afterOn: false }];
  const insertTarget = insertTargetOf(tokens);
  let into = -1;
  let pendingConflict = false;

  for (let i = 0; i < upto; i += 1) {
    const token = tokens[i];
    const top = frames[frames.length - 1];

    if (token.type === "word") {
      switch (token.value.toUpperCase()) {
        case "SELECT":
          top.clause = "select";
          top.select = i;
          top.afterOn = false;
          break;
        case "FROM":
        case "JOIN":
          top.clause = "from";
          top.afterOn = false;
          break;
        case "UPDATE":
          top.clause = "from";
          break;
        case "ON":
          if (word(tokens[i + 1]) === "CONFLICT") pendingConflict = true;
          else top.afterOn = true;
          break;
        case "WHERE":
          top.clause = "where";
          break;
        case "GROUP":
          top.clause = "group";
          break;
        case "ORDER":
          top.clause = "order";
          break;
        case "HAVING":
          top.clause = "having";
          break;
        case "SET":
          top.clause = "set";
          break;
        case "VALUES":
          top.clause = "values";
          break;
        case "RETURNING":
          top.clause = "select";
          break;
        case "UNION":
        case "INTERSECT":
        case "EXCEPT":
        case "WITH":
          top.clause = "other";
          break;
        case "INTO":
          into = i;
          break;
      }
    } else if (isPunct(token, "(")) {
      const previous = tokens[i - 1];
      const afterTarget =
        into !== -1 && (i - 1 === into + 1 || (i - 1 === into + 3 && isPunct(tokens[into + 2], ".")));
      if (afterTarget && identName(previous) !== null && word(tokens[into - 1]) === "INSERT") {
        frames.push({ clause: "insert-columns", open: i, select: null, afterOn: false });
      } else if (pendingConflict && word(previous) === "CONFLICT") {
        pendingConflict = false;
        frames.push({ clause: "on-conflict", open: i, select: null, afterOn: false });
      } else {
        // Una llamada o un agrupador: sigue en la misma cláusula que el nivel de afuera.
        frames.push({ clause: top.clause, open: i, select: null, afterOn: false });
      }
    } else if (isPunct(token, ")")) {
      if (frames.length > 1) frames.pop();
    }
  }

  const frame = frames[frames.length - 1];
  const before = tokens[upto - 1];
  const tableSlot =
    ["FROM", "JOIN", "LATERAL", "UPDATE", "ONLY"].includes(word(before)) ||
    (isPunct(before, ",") && frame.clause === "from" && !frame.afterOn);

  return { tokens, upto, clause: frame.clause, frame, tableSlot, insertTarget };
}

/** En qué cláusula de la sentencia está el cursor. */
export function clauseAt(sql: string, cursor: number): Clause {
  return analyze(sql, cursor).clause;
}

/** Reemplaza por espacios todo lo que está dentro de paréntesis anidados, sin mover posiciones. */
export function blankNested(text: string): string {
  const { tokens } = lex(text);
  const chars = text.split("");
  let depth = 0;
  for (const token of tokens) {
    if (isPunct(token, ")")) depth = Math.max(0, depth - 1);
    if (depth > 0) {
      for (let i = token.start; i < token.end; i += 1) if (chars[i] !== "\n") chars[i] = " ";
    }
    if (isPunct(token, "(")) depth += 1;
  }
  return chars.join("");
}

/** Palabras que terminan la lista de columnas de un `SELECT`. */
const ENDS_SELECT_LIST = new Set([
  "FROM", "WHERE", "GROUP", "ORDER", "HAVING", "LIMIT", "OFFSET", "FETCH", "UNION", "INTERSECT",
  "EXCEPT", "WINDOW", "FOR", "INTO", "RETURNING",
]);

/** Palabras que cierran una expresión y que, al final de un ítem, no pueden ser su alias. */
const NOT_AN_ALIAS = new Set(["END", "NULL", "TRUE", "FALSE", "ASC", "DESC", "FIRST", "LAST", "DISTINCT"]);

/** Parte la lista de ítems de un `SELECT` que empieza en `selectIndex`, hasta su `FROM` o su cierre. */
function selectItems(tokens: Token[], selectIndex: number): Token[][] {
  const items: Token[][] = [];
  let current: Token[] = [];
  let depth = 0;
  let i = selectIndex + 1;

  // `SELECT DISTINCT ON (…)` y `SELECT ALL`: no son columnas.
  if (word(tokens[i]) === "ALL") i += 1;
  else if (word(tokens[i]) === "DISTINCT") {
    i += 1;
    if (word(tokens[i]) === "ON" && isPunct(tokens[i + 1], "(")) {
      let nested = 0;
      for (; i < tokens.length; i += 1) {
        if (isPunct(tokens[i], "(")) nested += 1;
        else if (isPunct(tokens[i], ")")) {
          nested -= 1;
          if (nested === 0) {
            i += 1;
            break;
          }
        }
      }
    }
  }

  for (; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (isPunct(token, "(")) depth += 1;
    else if (isPunct(token, ")")) {
      if (depth === 0) break;
      depth -= 1;
    } else if (depth === 0 && ENDS_SELECT_LIST.has(word(token))) break;
    else if (depth === 0 && isPunct(token, ",")) {
      items.push(current);
      current = [];
      continue;
    }
    current.push(token);
  }
  if (current.length > 0) items.push(current);
  return items;
}

/** El alias que un ítem declara: con `AS`, o escrito pegado detrás de la expresión. */
function declaredAlias(item: Token[]): string | null {
  const last = item[item.length - 1];
  const name = identName(last);
  if (name === null || item.length < 2) return null;
  if (last.type === "word" && NOT_AN_ALIAS.has(last.value.toUpperCase()) && word(item[item.length - 2]) !== "AS") {
    return null;
  }
  const previous = item[item.length - 2];
  if (word(previous) === "AS") return name;
  // `expresión alias`: lo anterior tiene que cerrar una expresión, no seguir una operación.
  if (previous.type === "punct") return previous.value === ")" || previous.value === "]" ? name : null;
  return name;
}

/** Los alias que la lista `SELECT` de la sentencia del cursor declara: lo que `ORDER BY` puede nombrar. */
export function selectAliases(sql: string, cursor: number): string[] {
  const analysis = analyze(sql, cursor);
  const selectIndex = analysis.frame.select;
  if (selectIndex === null) return [];
  const aliases: string[] = [];
  for (const item of selectItems(analysis.tokens, selectIndex)) {
    const alias = declaredAlias(item);
    if (alias !== null && !aliases.includes(alias)) aliases.push(alias);
  }
  return aliases;
}

function relationFor(relations: SchemaRelation[], ref: TableRef): SchemaRelation | undefined {
  return relationForRef(relations, ref);
}

/**
 * Las columnas que produce un ítem de la lista, con el nombre con el que PostgreSQL las llama:
 * el alias, la última parte de `a.nombre` o el nombre de la función (`count(*)` se llama `count`).
 * Un `*` se resuelve contra las tablas del propio `SELECT`; si alguna no se conoce, esas columnas
 * faltan en vez de inventarse.
 */
function itemColumns(item: Token[], refs: TableRef[], relations: SchemaRelation[]): string[] {
  const alias = declaredAlias(item);
  if (alias !== null) return [alias];

  const last = item[item.length - 1];
  if (isPunct(last, "*")) {
    const qualifier = isPunct(item[item.length - 2], ".") ? identName(item[item.length - 3]) : null;
    const chosen =
      qualifier === null
        ? refs
        : refs.filter((ref) => (ref.alias ?? ref.name) === qualifier || ref.name === qualifier);
    return chosen.flatMap((ref) => relationFor(relations, ref)?.columns.map((c) => c.name) ?? []);
  }

  const name = identName(last);
  if (name !== null && item.length === 1) return [name];
  if (name !== null && isPunct(item[item.length - 2], ".")) return [name];
  // Una sola llamada sin alias: `count(*)` → `count`.
  if (isPunct(last, ")") && item.length >= 3 && identName(item[0]) !== null && isPunct(item[1], "(")) {
    return [identName(item[0]) as string];
  }
  return [];
}

/**
 * Las CTE de la sentencia del cursor como relaciones sintéticas, para completar sus columnas.
 *
 * `WITH ventas (a, b) AS (SELECT …)` ofrece las columnas de la lista `SELECT` —o las que el propio
 * `WITH` nombra, que mandan—. Son relaciones **solo para completar**: tienen `oid` negativo y
 * esquema vacío, y no tienen que llegar al `Ctrl`+clic ni al árbol, que esperan una tabla real.
 */
export function cteRelations(sql: string, cursor: number, relations: SchemaRelation[]): SchemaRelation[] {
  const { tokens } = analyze(sql, cursor);
  if (word(tokens[0]) !== "WITH") return [];

  const found: SchemaRelation[] = [];
  let i = 1;
  if (word(tokens[i]) === "RECURSIVE") i += 1;

  while (i < tokens.length) {
    const name = identName(tokens[i]);
    if (name === null) break;
    i += 1;

    let explicit: string[] = [];
    if (isPunct(tokens[i], "(")) {
      let j = i + 1;
      while (j < tokens.length && !isPunct(tokens[j], ")")) {
        const column = identName(tokens[j]);
        if (column !== null) explicit.push(column);
        j += 1;
      }
      i = j + 1;
    }
    if (word(tokens[i]) !== "AS") break;
    i += 1;
    if (word(tokens[i]) === "NOT") i += 1;
    if (word(tokens[i]) === "MATERIALIZED") i += 1;
    if (!isPunct(tokens[i], "(")) break;

    // El cierre del paréntesis del cuerpo; sin cerrar —se está escribiendo adentro— llega al final.
    let depth = 0;
    let close = tokens.length;
    for (let j = i; j < tokens.length; j += 1) {
      if (isPunct(tokens[j], "(")) depth += 1;
      else if (isPunct(tokens[j], ")")) {
        depth -= 1;
        if (depth === 0) {
          close = j;
          break;
        }
      }
    }
    const body = tokens.slice(i + 1, close);

    let columns: string[] = [];
    if (word(body[0]) === "SELECT") {
      const bodyText = blankNested(sql.slice(body[0].start, (body[body.length - 1] ?? body[0]).end));
      const refs = tablesInScope(bodyText, 0);
      const known = [...found, ...relations];
      columns = selectItems(body, 0).flatMap((item) => itemColumns(item, refs, known));
    }
    columns = columns.map((column, index) => explicit[index] ?? column);
    if (explicit.length > columns.length) columns = explicit;
    explicit = [];

    found.push({
      oid: -(found.length + 1),
      schema: "",
      name,
      columns: [...new Set(columns)].map((column) => ({ name: column, typeName: "" })),
    });

    i = close + 1;
    if (!isPunct(tokens[i], ",")) break;
    i += 1;
  }
  return found;
}

/**
 * La fuente de autocompletado, para sumarla a la del dialecto.
 *
 * Las relaciones llegan por función y no por valor: el esquema se pide después de abrir la pestaña,
 * y cambia entero al cambiar de base. Qué se ofrece depende de **dónde** está el cursor
 * (`clauseAt`): en la lista de columnas de un `INSERT` solo las que faltan escribir; después de
 * `ON CONFLICT (` las de la tabla; en `ORDER BY`/`GROUP BY`/`HAVING` además los alias del `SELECT`;
 * y `EXCLUDED.` ofrece las columnas de la tabla del `INSERT`.
 */
export function columnCompletion(relations: () => SchemaRelation[]) {
  return (context: CompletionContext): CompletionResult | null => {
    const sql = context.state.doc.toString();

    // `EXCLUDED.col`: la fuente del dialecto no la conoce, no es una tabla del catálogo.
    const excluded = context.matchBefore(/\bexcluded\s*\.\s*\w*/i);
    if (excluded) {
      const target = insertTargetRelation(analyze(sql, context.pos), relations());
      if (!target) return null;
      return {
        from: excluded.from + excluded.text.search(/\w*$/),
        options: target.columns.map((column) => ({
          label: column.name,
          detail: "excluded",
          type: "property",
          boost: 1,
        })),
        validFor: /^\w*$/,
      };
    }

    // Detrás de un punto ya contesta la fuente del dialecto, que sabe a qué tabla pertenece.
    const qualified = context.matchBefore(/\.\s*\w*/);
    if (qualified) return null;

    const word = context.matchBefore(/\w+/);
    if (!word && !context.explicit) return null;

    const analysis = analyze(sql, context.pos);
    const known = relations();
    const withCtes = [...known, ...cteRelations(sql, context.pos, known)];

    let options: Array<{ label: string; detail: string; type: string; boost: number }> = [];

    if (analysis.clause === "insert-columns" || analysis.clause === "on-conflict") {
      const target = insertTargetRelation(analysis, withCtes);
      if (!target) return null;
      // Lo ya escrito en la lista no se vuelve a ofrecer: una columna repetida la rechaza el servidor.
      const written = new Set<string>();
      if (analysis.clause === "insert-columns") {
        for (let i = analysis.frame.open + 1; i < analysis.upto; i += 1) {
          const name = identName(analysis.tokens[i]);
          if (name !== null) written.add(name.toLowerCase());
        }
      }
      options = target.columns
        .filter((column) => !written.has(column.name.toLowerCase()))
        .map((column) => ({ label: column.name, detail: target.name, type: "property", boost: 1 }));
    } else {
      options = columnOptions(withCtes, tablesInScope(sql, context.pos)).map((option) => ({
        ...option,
        type: "property",
        boost: 1,
      }));
      if (["order", "group", "having"].includes(analysis.clause)) {
        for (const alias of selectAliases(sql, context.pos)) {
          options.push({ label: alias, detail: "alias", type: "variable", boost: 2 });
        }
      }
      if (analysis.tableSlot) {
        for (const cte of withCtes.filter((relation) => relation.oid < 0)) {
          options.push({ label: cte.name, detail: "CTE", type: "class", boost: 1 });
        }
      }
    }
    if (options.length === 0) return null;

    return {
      from: word?.from ?? context.pos,
      // Por encima de las palabras clave del dialecto: escrito el `FROM`, lo que se busca al
      // teclear tres letras es una columna de esa tabla y no `CURRENT_TIMESTAMP`.
      options,
      validFor: /^\w*$/,
    };
  };
}

function insertTargetRelation(analysis: Analysis, relations: SchemaRelation[]): SchemaRelation | null {
  const target = analysis.insertTarget;
  if (!target) return null;
  return (
    relations.find(
      (relation) => relation.name === target.name && (target.schema === null || relation.schema === target.schema),
    ) ?? null
  );
}

/** Lo que puede formar un identificador de SQL sin comillas. */
const IDENT = /[\p{L}\p{N}_]/u;

/** Un nombre simple, o `esquema.nombre` si el punto queda pegado a la izquierda. */
export interface QualifiedName {
  schema: string | null;
  name: string;
}

/** Lo mismo, con dónde empieza y termina `name` en el texto: lo que necesita el `hover` para saber
 *  hasta dónde vale el globo. `relationAt` y `hoverInfo` no lo piden porque no les hace falta. */
export interface WordAt extends QualifiedName {
  from: number;
  to: number;
}

/**
 * El identificador que toca `pos`, para el `Ctrl`+clic que revela una tabla en el árbol y para el
 * `hover` que muestra su tipo y su comentario.
 *
 * Se expande en las dos direcciones —no solo hacia atrás, como `wordBefore`— porque un clic o un
 * `hover` caen en cualquier punto de la palabra, no en su final.
 */
export function qualifiedNameAt(text: string, pos: number): WordAt | null {
  let from = pos;
  while (from > 0 && IDENT.test(text[from - 1])) from -= 1;
  let to = pos;
  while (to < text.length && IDENT.test(text[to])) to += 1;
  if (from === to) return null;

  const name = text.slice(from, to);
  if (text[from - 1] !== ".") return { schema: null, name, from, to };

  let schemaFrom = from - 1;
  while (schemaFrom > 0 && IDENT.test(text[schemaFrom - 1])) schemaFrom -= 1;
  if (schemaFrom === from - 1) return { schema: null, name, from, to };

  return { schema: text.slice(schemaFrom, from - 1), name, from, to };
}

/**
 * A qué relación apunta un nombre escrito en el editor.
 *
 * Prioriza lo que está a la vista en el `FROM`/`JOIN` de la sentencia —un alias o un nombre sin
 * calificar casi siempre se refieren a eso— antes de buscar por nombre suelto en todo lo que trajo
 * el catálogo, donde el mismo nombre puede repetirse en dos esquemas. Sin coincidencia clara,
 * `null`: un `Ctrl`+clic que abre la tabla equivocada es peor que uno que no hace nada.
 */
export function relationAt(
  relations: SchemaRelation[],
  refs: TableRef[],
  qualified: QualifiedName,
): SchemaRelation | null {
  if (qualified.schema !== null) {
    return (
      relations.find(
        (relation) => relation.schema === qualified.schema && relation.name === qualified.name,
      ) ?? null
    );
  }

  const ref =
    refs.find((candidate) => candidate.alias === qualified.name) ??
    refs.find((candidate) => candidate.name === qualified.name);
  if (ref) {
    return (
      relations.find(
        (relation) => relation.name === ref.name && (ref.schema === null || relation.schema === ref.schema),
      ) ?? null
    );
  }

  const matches = relations.filter((relation) => relation.name === qualified.name);
  return matches.length === 1 ? matches[0] : null;
}

function relationForRef(relations: SchemaRelation[], ref: TableRef): SchemaRelation | undefined {
  return relations.find(
    (relation) => relation.name === ref.name && (ref.schema === null || relation.schema === ref.schema),
  );
}

export type HoverInfo =
  | { kind: "column"; table: string; column: RelationColumn }
  | { kind: "table"; relation: SchemaRelation };

/**
 * Qué mostrar en el `hover` de un identificador: tipo y comentario si es una columna, o el
 * comentario de la tabla si no lo es. `null` cuando no hay nada que decir —una tabla sin
 * `COMMENT ON` no aporta nada que el árbol no muestre ya, y mostrar un globo vacío es peor que no
 * mostrar nada—.
 */
export function hoverInfo(
  relations: SchemaRelation[],
  refs: TableRef[],
  qualified: QualifiedName,
): HoverInfo | null {
  if (qualified.schema !== null) {
    // El prefijo puede ser un alias (`c.total`) o un esquema (`ventas.clientes`): se prueba como
    // alias primero, porque un esquema nunca hace de alias de nada.
    const owner = refs.find((ref) => ref.alias === qualified.schema);
    if (owner) {
      const relation = relationForRef(relations, owner);
      const column = relation?.columns.find((candidate) => candidate.name === qualified.name);
      if (relation && column) return { kind: "column", table: owner.alias ?? relation.name, column };
    }

    const table = relations.find(
      (relation) => relation.schema === qualified.schema && relation.name === qualified.name,
    );
    return table?.comment ? { kind: "table", relation: table } : null;
  }

  for (const ref of refs) {
    const relation = relationForRef(relations, ref);
    const column = relation?.columns.find((candidate) => candidate.name === qualified.name);
    if (relation && column) return { kind: "column", table: ref.alias ?? relation.name, column };
  }

  const relation = relationAt(relations, refs, qualified);
  return relation && relation.comment ? { kind: "table", relation } : null;
}
