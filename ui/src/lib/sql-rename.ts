/**
 * Renombrar un alias de tabla (o una CTE) en toda su sentencia.
 *
 * Cambiar `c` por `cli` en un `JOIN` de seis tablas a mano es dejar un `c.` olvidado, y ese error
 * recién aparece al ejecutar. Se encuentran las apariciones leyendo el texto (tokenizador de
 * `statement-risk.ts`: no mira adentro de cadenas, comentarios ni `$$ … $$`) y se limita a **una
 * sentencia**, la delimitada por `;`.
 *
 * Límite conocido: el alcance es la sentencia entera, no el de cada subconsulta. Si el mismo alias
 * se declara en dos subconsultas distintas, se renombran las dos: el diálogo muestra cuántas
 * apariciones son antes de aplicar. Lo que no es un alias —el nombre de una tabla usado sin alias—
 * no se renombra: eso cambiaría el objeto, no el texto.
 */

import { isReserved } from "./sql-ident";
import { lex, type Token } from "./statement-risk";

export interface AliasOccurrence {
  from: number;
  to: number;
}

export interface AliasAt {
  name: string;
  kind: "alias" | "cte";
  occurrences: AliasOccurrence[];
}

const word = (token: Token | undefined) => (token?.type === "word" ? token.value.toUpperCase() : "");
const isPunct = (token: Token | undefined, value: string) =>
  token?.type === "punct" && token.value === value;

/** El nombre que escribe un identificador. */
function identName(token: Token | undefined): string | null {
  if (token?.type === "word" && !/^[0-9]/.test(token.value)) return token.value;
  if (token?.type === "opaque" && token.value.startsWith('"')) return token.inner;
  return null;
}

/** PostgreSQL pliega a minúsculas lo que no lleva comillas; con comillas, el nombre es exacto. */
function keyOf(token: Token): string {
  return token.type === "opaque" ? token.inner : token.value.toLowerCase();
}

/** Lo que, detrás de una tabla, no es un alias sino lo que sigue en la sentencia. */
const EXTRA_NOT_ALIAS = new Set(["SET", "VALUES", "OVERRIDING", "LATERAL", "ONLY"]);
const notAlias = (token: Token) =>
  token.type === "word" && (isReserved(token.value) || EXTRA_NOT_ALIAS.has(token.value.toUpperCase()));

interface Definition {
  key: string;
  kind: "alias" | "cte";
  token: Token;
}

interface Analysis {
  definitions: Definition[];
  /** Usos como calificador (`x.col`) y, para una CTE, como nombre de tabla. */
  uses: Array<{ key: string; token: Token; asTable: boolean }>;
  /** Tablas nombradas sin alias: sus nombres también están ocupados en la sentencia. */
  tableKeys: Set<string>;
  tokens: Token[];
}

function matchingClose(tokens: Token[], open: number, hi: number): number {
  let depth = 0;
  for (let j = open; j < hi; j += 1) {
    if (isPunct(tokens[j], "(")) depth += 1;
    else if (isPunct(tokens[j], ")")) {
      depth -= 1;
      if (depth === 0) return j;
    }
  }
  return hi;
}

function analyze(text: string, pos: number): Analysis {
  const all = lex(text).tokens;

  // La sentencia del cursor.
  let lo = 0;
  let hi = all.length;
  for (let i = 0; i < all.length; i += 1) {
    if (!isPunct(all[i], ";")) continue;
    if (all[i].start >= pos) {
      hi = i;
      break;
    }
    lo = i + 1;
  }
  const tokens = all.slice(lo, hi);
  const end = tokens.length;

  const definitions: Definition[] = [];
  const tableUses: Array<{ key: string; token: Token }> = [];
  const tableKeys = new Set<string>();

  // --- CTE: WITH [RECURSIVE] x [(cols)] AS [NOT] [MATERIALIZED] ( … ), y AS ( … ) -----------------
  if (word(tokens[0]) === "WITH") {
    let i = 1;
    if (word(tokens[i]) === "RECURSIVE") i += 1;
    while (i < end) {
      const nameToken = tokens[i];
      if (identName(nameToken) === null) break;
      definitions.push({ key: keyOf(nameToken), kind: "cte", token: nameToken });
      i += 1;
      if (isPunct(tokens[i], "(")) i = matchingClose(tokens, i, end) + 1;
      if (word(tokens[i]) !== "AS") break;
      i += 1;
      if (word(tokens[i]) === "NOT") i += 1;
      if (word(tokens[i]) === "MATERIALIZED") i += 1;
      if (!isPunct(tokens[i], "(")) break;
      i = matchingClose(tokens, i, end) + 1;
      if (!isPunct(tokens[i], ",")) break;
      i += 1;
    }
  }

  // --- alias de tabla: FROM / JOIN / UPDATE / INTO ------------------------------------------------
  for (let i = 0; i < end; i += 1) {
    const keyword = word(tokens[i]);
    if (!["FROM", "JOIN", "UPDATE", "INTO"].includes(keyword)) continue;
    // `DELETE FROM`, `FOR UPDATE`, `ON UPDATE`: solo el `UPDATE` que abre una sentencia lleva tabla.
    if (keyword === "UPDATE" && !(i === 0 || ["WITH", "AS", "(", ","].includes(word(tokens[i - 1])) || isPunct(tokens[i - 1], ")"))) {
      continue;
    }

    let j = i + 1;
    let more = true;
    while (more) {
      more = false;
      while (["LATERAL", "ONLY"].includes(word(tokens[j]))) j += 1;

      let tableToken: Token | undefined;
      let qualified = false;
      if (isPunct(tokens[j], "(")) {
        // Una subconsulta: el alias va después del paréntesis de cierre.
        j = matchingClose(tokens, j, end) + 1;
      } else if (identName(tokens[j]) !== null) {
        tableToken = tokens[j];
        j += 1;
        while (isPunct(tokens[j], ".") && identName(tokens[j + 1]) !== null) {
          qualified = true;
          tableToken = tokens[j + 1];
          j += 2;
        }
        // Una función en el FROM: `generate_series(1, 3) AS g`.
        if (isPunct(tokens[j], "(")) {
          tableToken = undefined;
          j = matchingClose(tokens, j, end) + 1;
        }
      } else {
        break;
      }

      let hasAs = false;
      if (word(tokens[j]) === "AS") {
        hasAs = true;
        j += 1;
      }
      const candidate = tokens[j];
      const aliased = candidate !== undefined && identName(candidate) !== null && (hasAs || !notAlias(candidate));
      if (aliased) {
        definitions.push({ key: keyOf(candidate), kind: "alias", token: candidate });
        j += 1;
        // Lista de columnas del alias: `AS t (a, b)`.
        if (isPunct(tokens[j], "(")) j = matchingClose(tokens, j, end) + 1;
      } else if (tableToken !== undefined) {
        // Sin alias, el nombre de la tabla es lo que se usa para calificar: queda ocupado.
        tableKeys.add(keyOf(tableToken));
      }
      // Con o sin alias, es un uso del nombre: si ese nombre es una CTE, se renombra con ella.
      if (tableToken !== undefined && !qualified) tableUses.push({ key: keyOf(tableToken), token: tableToken });

      // `FROM a, b`: la coma de primer nivel sigue la lista; en un JOIN o un INSERT INTO no.
      if (keyword === "FROM" && isPunct(tokens[j], ",")) {
        j += 1;
        more = true;
      }
    }
  }

  // --- usos como calificador: x.columna --------------------------------------------------------
  const uses: Analysis["uses"] = tableUses.map((use) => ({ ...use, asTable: true }));
  for (let i = 0; i < end; i += 1) {
    const token = tokens[i];
    if (identName(token) === null) continue;
    if (isPunct(tokens[i + 1], ".") && !isPunct(tokens[i - 1], ".")) {
      uses.push({ key: keyOf(token), token, asTable: false });
    }
  }

  return { definitions, uses, tableKeys, tokens };
}

/** Las apariciones de cada nombre declarado, indexadas por clave. */
function occurrencesByKey(analysis: Analysis) {
  const byKey = new Map<string, { kind: "alias" | "cte"; tokens: Map<number, Token> }>();
  for (const definition of analysis.definitions) {
    const entry = byKey.get(definition.key) ?? { kind: definition.kind, tokens: new Map() };
    entry.tokens.set(definition.token.start, definition.token);
    byKey.set(definition.key, entry);
  }
  for (const use of analysis.uses) {
    const entry = byKey.get(use.key);
    if (!entry) continue;
    // Una tabla usada por su nombre solo es una aparición si ese nombre es una CTE: para un alias
    // es otra cosa (la tabla real que casualmente se llama igual).
    if (use.asTable && entry.kind !== "cte") continue;
    entry.tokens.set(use.token.start, use.token);
  }
  return byKey;
}

export function aliasAt(text: string, pos: number): AliasAt | null {
  const analysis = analyze(text, pos);
  const byKey = occurrencesByKey(analysis);

  for (const [key, entry] of byKey) {
    for (const token of entry.tokens.values()) {
      if (token.start <= pos && pos <= token.end) {
        return {
          name: identName(token) ?? key,
          kind: entry.kind,
          occurrences: [...entry.tokens.values()]
            .sort((a, b) => a.start - b.start)
            .map((t) => ({ from: t.start, to: t.end })),
        };
      }
    }
  }
  return null;
}

/** Un cambio por aparición, con el nombre tal como lo escribió el usuario (ya validado). */
export function renameAlias(
  occurrences: AliasOccurrence[],
  newName: string,
): Array<{ from: number; to: number; insert: string }> {
  return occurrences.map((occurrence) => ({ ...occurrence, insert: newName }));
}

/**
 * Por qué `newName` no sirve como alias del que está bajo el cursor, o `null` si sirve.
 *
 * Sin comillas tiene que ser un identificador que el servidor lea tal cual y no una palabra
 * reservada; con comillas dobles vale cualquier texto. Tampoco puede chocar con otro alias, una CTE
 * o una tabla de la sentencia: dos tablas con el mismo nombre dan «el nombre de tabla … se
 * especificó más de una vez» recién al ejecutar.
 */
export function validateAliasName(text: string, pos: number, newName: string): string | null {
  const name = newName.trim();
  if (name === "") return "Escribe un nombre para el alias.";

  let key: string;
  if (name.startsWith('"')) {
    if (!/^"(?:[^"]|"")+"$/.test(name)) {
      return "El nombre entre comillas dobles tiene que abrir y cerrar con comillas, sin comillas sueltas adentro.";
    }
    key = name.slice(1, -1).replace(/""/g, '"');
  } else {
    if (!/^[\p{L}_][\p{L}\p{N}_$]*$/u.test(name)) {
      return "Sin comillas, el nombre empieza con una letra o _ y solo lleva letras, números, _ y $. Para otro formato, ponlo entre comillas dobles.";
    }
    if (isReserved(name)) {
      return `«${name}» es una palabra reservada de SQL. Elige otro nombre o ponlo entre comillas dobles.`;
    }
    key = name.toLowerCase();
  }

  const analysis = analyze(text, pos);
  const byKey = occurrencesByKey(analysis);
  let current: string | null = null;
  for (const [candidate, entry] of byKey) {
    for (const token of entry.tokens.values()) {
      if (token.start <= pos && pos <= token.end) current = candidate;
    }
  }
  if (current === null) return "El cursor no está sobre un alias.";
  if (key === current) return "El nombre no cambió.";

  const taken = new Set<string>([...byKey.keys()].filter((candidate) => candidate !== current));
  for (const table of analysis.tableKeys) taken.add(table);
  if (taken.has(key)) return `«${name}» ya se usa en esta sentencia para otra tabla o alias.`;
  return null;
}
