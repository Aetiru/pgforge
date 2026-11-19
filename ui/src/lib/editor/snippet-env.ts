/**
 * Lo que una expansión de abreviatura necesita saber del lugar donde se expande, sin tocar el
 * editor: los valores automáticos (`${hoy}`, `${base}`…), las opciones de las variables de catálogo
 * y qué abreviaturas tienen atajo propio. Puro y con Vitest; `sql-snippet.ts` y `SqlEditor` solo lo
 * usan.
 */

import type { SchemaRelation, Snippet } from "../ipc";
import { normalizeCombo } from "../snippet-check";
import { clockContext, type AutoName } from "../snippet-vars";
import { columnOptions, tablesInScope } from "../sql-complete";
import { quoteIdent } from "../sql-ident";

/** Dónde está escribiendo el usuario: de acá salen el alcance y los automáticos. */
export interface SnippetContext {
  profileId: string | null;
  database: string | null;
  /** Nombre del servidor tal como lo ve el usuario. */
  server: string | null;
  /** El rol con el que se conectó el perfil: la aplicación no tiene cuentas propias. */
  user: string | null;
}

/**
 * Los automáticos que se pueden llenar sin preguntar nada. Un dato que no se conoce **no entra**:
 * `fillAutomatic` deja entonces el nombre como hueco común, que es mejor que insertar un texto
 * vacío donde el usuario esperaba un valor. `seleccion` y `portapapeles` no están: dependen del
 * momento exacto de la expansión y los pone quien expande.
 */
export function autoContext(
  context: SnippetContext,
  now: Date = new Date(),
): Partial<Record<AutoName, string>> {
  const out: Partial<Record<AutoName, string>> = { ...clockContext(now) };
  if (context.user) out.usuario = context.user;
  if (context.database) out.base = context.database;
  if (context.server) out.servidor = context.server;
  return out;
}

/**
 * Lo seleccionado al expandir. Una selección vacía no cuenta: la abreviatura recién escrita no es
 * «lo seleccionado», y sin selección `${seleccion}` tiene que seguir siendo un hueco donde escribir.
 */
export function withSelection(
  base: Partial<Record<AutoName, string>>,
  selection: string | undefined,
): Partial<Record<AutoName, string>> {
  return selection ? { ...base, seleccion: selection } : base;
}

/**
 * Espera la lectura del portapapeles, pero no más de `ms`: en una ventana de escritorio el sistema
 * puede pedir permiso, y una expansión que se queda esperando una respuesta que nunca llega es peor
 * que dejar el hueco sin llenar. Cualquier fallo vale lo mismo que el tiempo vencido: `null`.
 */
export function readWithTimeout(read: () => Promise<string>, ms: number): Promise<string | null> {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), ms);
    let pending: Promise<string>;
    try {
      pending = read();
    } catch {
      clearTimeout(timer);
      resolve(null);
      return;
    }
    pending.then(
      (text) => {
        clearTimeout(timer);
        resolve(text);
      },
      () => {
        clearTimeout(timer);
        resolve(null);
      },
    );
  });
}

/** `esquema.nombre` calificado y citado donde hace falta: lo que va a quedar escrito en el SQL. */
export function qualifiedName(relation: Pick<SchemaRelation, "schema" | "name">): string {
  return relation.schema ? `${quoteIdent(relation.schema)}.${quoteIdent(relation.name)}` : quoteIdent(relation.name);
}

/** Las tablas del catálogo para la variable `@tabla`, sin las sintéticas de una CTE (`oid < 0`). */
export function tableChoices(relations: SchemaRelation[]): string[] {
  return relations
    .filter((relation) => relation.oid >= 0)
    .map(qualifiedName)
    .sort((a, b) => a.localeCompare(b, "es", { sensitivity: "base" }));
}

/**
 * Las columnas para la variable `@columna`: primero las de las tablas que la sentencia del cursor
 * tiene a la vista —que es casi siempre lo que se quiere—, y si no hay ninguna (script en blanco,
 * tabla que el catálogo no conoce) las de todo el catálogo, sin repetir.
 */
export function columnChoices(relations: SchemaRelation[], text: string, cursor: number): string[] {
  const scoped = columnOptions(relations, tablesInScope(text, cursor)).map((option) => option.label);
  const names = scoped.length
    ? scoped
    : relations.filter((relation) => relation.oid >= 0).flatMap((relation) => relation.columns.map((c) => c.name));
  return [...new Set(names)].sort((a, b) => a.localeCompare(b, "es", { sensitivity: "base" }));
}

export interface ShortcutEntry {
  /** La combinación tal cual se la escribió: es la que entiende el keymap de CodeMirror. */
  combo: string;
  snippet: Snippet;
}

/**
 * Las abreviaturas con atajo propio que se pueden registrar en el keymap.
 *
 * `taken` son las combinaciones de la aplicación y del editor: **ganan ellas**. El atajo de una
 * abreviatura que coincide con uno incorporado calla en vez de pisarlo, porque quien lo asignó pudo
 * no saber que ya significaba otra cosa y perder `Ctrl+Enter` por una abreviatura sería peor que
 * que no funcione la abreviatura (el diálogo ya avisa del choque al asignarlo). Entre dos
 * abreviaturas con la misma combinación vale la primera de la lista: el núcleo ya no deja guardar
 * esa situación, pero un `snippets.json` editado a mano sí puede traerla.
 */
export function shortcutEntries(items: readonly Snippet[], taken: Iterable<string | null>): ShortcutEntry[] {
  const used = new Set<string>();
  for (const combo of taken) {
    const key = normalizeCombo(combo);
    if (key) used.add(key);
  }
  const out: ShortcutEntry[] = [];
  for (const snippet of items) {
    const key = normalizeCombo(snippet.shortcut);
    // Sin Ctrl, Alt o Meta sería una tecla suelta: se comería el texto que se escribe.
    if (!snippet.shortcut || !key || !/(^|-)(ctrl|alt|meta)-/.test(key) || used.has(key)) continue;
    used.add(key);
    out.push({ combo: snippet.shortcut, snippet });
  }
  return out;
}
