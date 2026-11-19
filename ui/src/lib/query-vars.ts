/**
 * Variables de una consulta: `{{nombre}}`.
 *
 * Al ejecutar, la aplicación pregunta el valor de cada nombre distinto y reemplaza el texto **tal
 * cual**, sin comillas ni escapes: el valor puede ser un número, un identificador o un trozo de
 * SQL, y quien escribe la consulta pone las comillas donde las quiere (`'{{estado}}'`). Es la misma
 * frontera de confianza que el editor —lo ejecuta el propio usuario con sus privilegios—, y por eso
 * la confirmación de producción mira el SQL **ya armado**, no el que tenía las llaves.
 *
 * No se tocan los comentarios: una variable comentada no se pregunta. Adentro de una cadena sí
 * vale, porque `'%{{texto}}%'` es el uso más común.
 *
 * Todo es texto y puro, con Vitest: el editor y `QueryTab` solo lo usan.
 */

export interface VarUse {
  name: string;
  /** Posición del `{{` en el texto, y de lo que sigue al `}}`. */
  start: number;
  end: number;
}

const OPEN = /\{\{\s*([\p{L}_][\p{L}\p{N}_]*)\s*\}\}/uy;

/** Las apariciones de `{{nombre}}` fuera de comentarios, en orden. */
export function findVars(sql: string): VarUse[] {
  if (!sql.includes("{{")) return [];

  const uses: VarUse[] = [];
  let i = 0;
  let inString = false;
  let inIdent = false;
  let blockDepth = 0;

  while (i < sql.length) {
    const char = sql[i];
    const next = sql[i + 1];

    if (blockDepth > 0) {
      if (char === "/" && next === "*") {
        blockDepth++;
        i += 2;
      } else if (char === "*" && next === "/") {
        blockDepth--;
        i += 2;
      } else i++;
      continue;
    }

    if (!inString && !inIdent) {
      if (char === "-" && next === "-") {
        const end = sql.indexOf("\n", i);
        if (end === -1) break;
        i = end + 1;
        continue;
      }
      if (char === "/" && next === "*") {
        blockDepth = 1;
        i += 2;
        continue;
      }
    }

    if (char === "'" && !inIdent) inString = !inString;
    else if (char === '"' && !inString) inIdent = !inIdent;
    else if (char === "{" && next === "{") {
      OPEN.lastIndex = i;
      const match = OPEN.exec(sql);
      if (match) {
        uses.push({ name: match[1], start: i, end: i + match[0].length });
        i += match[0].length;
        continue;
      }
    }
    i++;
  }

  return uses;
}

/** Los nombres sin repetir, en el orden en que aparecen: se pregunta una vez por nombre. */
export function varNames(uses: readonly VarUse[]): string[] {
  return [...new Set(uses.map((use) => use.name))];
}

/** El texto con cada variable reemplazada. Un nombre sin valor queda vacío. */
export function fillVars(sql: string, uses: readonly VarUse[], values: Record<string, string>): string {
  let out = "";
  let at = 0;
  for (const use of uses) {
    out += sql.slice(at, use.start) + (values[use.name] ?? "");
    at = use.end;
  }
  return out + sql.slice(at);
}

/**
 * Convierte una posición del texto ya armado a la del texto escrito. El servidor señala el error
 * sobre lo que recibió, y la marca del editor tiene que caer sobre lo que está escrito: sin esto,
 * un valor largo antes del error corre la marca. Una posición adentro de un valor cae en su `{{…}}`.
 */
export function unmapper(
  uses: readonly VarUse[],
  values: Record<string, string>,
): (position: number) => number {
  return (position) => {
    let shift = 0;
    for (const use of uses) {
      const filledStart = use.start + shift;
      const length = (values[use.name] ?? "").length;
      if (position < filledStart) break;
      if (position < filledStart + length) return use.start;
      shift += length - (use.end - use.start);
    }
    return position - shift;
  };
}

/** Lo recordado de la última vez, saneado: solo pares nombre → texto. */
export function parseRemembered(raw: string | null): Record<string, string> {
  if (!raw) return {};
  try {
    const data: unknown = JSON.parse(raw);
    if (typeof data !== "object" || data === null || Array.isArray(data)) return {};
    return Object.fromEntries(
      Object.entries(data).filter(([, value]) => typeof value === "string"),
    ) as Record<string, string>;
  } catch {
    return {};
  }
}
