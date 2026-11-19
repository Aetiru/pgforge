/**
 * Variables de una abreviatura.
 *
 * Conviven con los huecos de CodeMirror sin pisarlos. `${tabla}` (sin nada más) sigue siendo un
 * hueco por el que se salta con el tabulador, `${1:algo}` (número) sigue siendo un hueco numerado, y
 * lo que lleva un **nombre** más alguna de estas marcas es una variable que se pregunta en un
 * diálogo antes de insertar. Así las abreviaturas que ya existen no cambian de comportamiento.
 *
 *  - `${nombre:valor}`                  texto, con valor por omisión
 *  - `${estado|activo|baja|pendiente}`  lista: la primera opción es el valor por omisión
 *  - `${nombre@fecha}` / `@tabla` / `@columna` / `@numero`, con `:valor` opcional: el diálogo
 *    elige el control (selector de fecha, buscador de tablas o de columnas del catálogo, número)
 *
 * Además hay **valores automáticos**, que se llenan solos y no se preguntan: `${hoy}`, `${ahora}`,
 * `${usuario}`, `${base}`, `${servidor}`, `${seleccion}` y `${portapapeles}`. Son nombres
 * reservados: un hueco plano que ya se llamara así en una abreviatura vieja pasa a llenarse solo.
 * (`${base:x}` con dos puntos sigue siendo una variable común: el reservado es solo el hueco pelado.)
 *
 * Todo es texto y puro: el diálogo y el editor solo lo usan. El orden de uso es siempre
 * `fillAutomatic` → `fillVariables`: lo primero inserta valores ya escapados de llaves (la
 * selección puede traer `${` adentro) y lo segundo no los vuelve a leer.
 */

/** Un hueco o variable completo: `${…}` sin llaves adentro (el escape `\{` no cuenta como llave). */
const HOLE = /\$\{[^{}]*\}/g;

const NAME = /^([\p{L}_][\p{L}\p{N}_]*)(.*)$/su;

export type VariableKind = "text" | "list" | "date" | "table" | "column" | "number";

export interface SnippetVariable {
  name: string;
  /** El valor con el que arranca el campo del diálogo. */
  default: string;
  kind: VariableKind;
  /** Solo en las de lista: las opciones, en el orden escrito. */
  options?: string[];
}

const TYPES: Record<string, VariableKind> = {
  fecha: "date",
  tabla: "table",
  columna: "column",
  numero: "number",
};

export type AutoName =
  | "hoy"
  | "ahora"
  | "usuario"
  | "base"
  | "servidor"
  | "seleccion"
  | "portapapeles";

const AUTO_NAMES: readonly AutoName[] = [
  "hoy",
  "ahora",
  "usuario",
  "base",
  "servidor",
  "seleccion",
  "portapapeles",
];

const isAuto = (inner: string): inner is AutoName => (AUTO_NAMES as readonly string[]).includes(inner);

const pad = (n: number) => String(n).padStart(2, "0");

/** `YYYY-MM-DD` en hora local: la fecha que el usuario ve en su reloj, no la de Greenwich. */
export function localDate(now: Date = new Date()): string {
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** `YYYY-MM-DD HH:MM:SS` en hora local. */
export function localTimestamp(now: Date = new Date()): string {
  return `${localDate(now)} ${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
}

/** Los dos automáticos que no dependen de nadie más que del reloj. */
export function clockContext(now: Date = new Date()): Pick<Record<AutoName, string>, "hoy" | "ahora"> {
  return { hoy: localDate(now), ahora: localTimestamp(now) };
}

/** Escapa las llaves con `\{` y `\}`: lo único que el intérprete de CodeMirror reconoce como escape. */
function escapeBraces(value: string): string {
  return value.replace(/[{}]/g, "\\$&");
}

/**
 * Lee lo de adentro de un `${…}` como variable, o `null` si es un hueco común (o un automático).
 * El valor por omisión de una fecha sin valor escrito es la de hoy: el selector arranca ahí.
 */
function parseVariable(inner: string, now: Date): SnippetVariable | null {
  if (isAuto(inner)) return null;
  const match = NAME.exec(inner);
  if (!match) return null;
  const [, name, rest] = match;

  if (rest.startsWith(":")) return { name, default: rest.slice(1), kind: "text" };

  if (rest.startsWith("|")) {
    const options = rest.slice(1).split("|");
    return { name, default: options[0], kind: "list", options };
  }

  const typed = /^@(fecha|tabla|columna|numero)(?::(.*))?$/su.exec(rest);
  if (typed) {
    const kind = TYPES[typed[1]];
    const fallback = kind === "date" ? localDate(now) : kind === "number" ? "0" : "";
    return { name, default: typed[2] ?? fallback, kind };
  }
  return null;
}

/**
 * Las variables del cuerpo, sin repetir y en el orden en que aparecen. Si una se repite, vale la
 * primera declaración —su tipo y su valor por omisión—: se pregunta una sola vez y rellena todas.
 */
export function variablesOf(body: string, now: Date = new Date()): SnippetVariable[] {
  const seen = new Map<string, SnippetVariable>();
  for (const match of body.matchAll(HOLE)) {
    const variable = parseVariable(match[0].slice(2, -1), now);
    if (variable && !seen.has(variable.name)) seen.set(variable.name, variable);
  }
  return [...seen.values()];
}

/**
 * El cuerpo con cada variable reemplazada por lo que se escribió (o su valor por omisión si falta).
 *
 * El valor va a pasar por el intérprete de plantillas de CodeMirror, que lee `${…}` como hueco: las
 * llaves se escapan con `\{` y `\}` para que un valor con llaves se inserte literal y no cree
 * huecos. Los huecos comunes y los automáticos se dejan como están.
 */
export function fillVariables(
  body: string,
  values: Record<string, string>,
  now: Date = new Date(),
): string {
  const first = new Map(variablesOf(body, now).map((variable) => [variable.name, variable]));
  return body.replace(HOLE, (hole) => {
    const variable = parseVariable(hole.slice(2, -1), now);
    if (!variable) return hole;
    const value = values[variable.name] ?? first.get(variable.name)?.default ?? variable.default;
    return escapeBraces(value);
  });
}

/** Los automáticos que el cuerpo usa, sin repetir y en el orden en que aparecen. */
export function autoVariablesOf(body: string): AutoName[] {
  const seen: AutoName[] = [];
  for (const match of body.matchAll(HOLE)) {
    const inner = match[0].slice(2, -1);
    if (isAuto(inner) && !seen.includes(inner)) seen.push(inner);
  }
  return seen;
}

/**
 * Llena los automáticos que `ctx` trae. Un valor ausente **no se borra**: queda como hueco común
 * (`${seleccion}` sin selección es un lugar donde escribir, no un texto vacío).
 *
 * El valor se escapa de llaves igual que en `fillVariables`: `${seleccion}` puede traer una
 * sentencia entera, con `${` adentro, y sin escape el editor la leería como huecos.
 */
export function fillAutomatic(body: string, ctx: Partial<Record<AutoName, string>>): string {
  return body.replace(HOLE, (hole) => {
    const inner = hole.slice(2, -1);
    if (!isAuto(inner)) return hole;
    const value = ctx[inner];
    return value === undefined ? hole : escapeBraces(value);
  });
}

const SAMPLE: Record<AutoName, string> = {
  hoy: "",
  ahora: "",
  usuario: "usuario",
  base: "base",
  servidor: "servidor",
  seleccion: "SELECT 1",
  portapapeles: "valor",
};

/**
 * Cómo quedaría expandido **sin preguntar nada**: lo que ve el usuario mientras escribe el cuerpo.
 *
 * Las variables valen su valor por omisión (la lista, su primera opción; una fecha, hoy; una tabla o
 * columna, el nombre de la variable, porque no hay catálogo a mano; un número, 0), los huecos
 * `${tabla}` valen su nombre, `${*}` un asterisco, `${}` y `${1}` nada, `${1:x}` su texto, y los
 * automáticos lo de `sample` o un ejemplo razonable. Es una sola pasada sobre el cuerpo: lo que se
 * inserta no se vuelve a leer, así que un valor con `${` no dispara nada.
 */
export function previewBody(
  body: string,
  sample: Partial<Record<AutoName, string>> = {},
  now: Date = new Date(),
): string {
  const clock = clockContext(now);
  const autos: Record<AutoName, string> = { ...SAMPLE, ...clock, ...sample };
  const first = new Map(variablesOf(body, now).map((variable) => [variable.name, variable]));
  // Entre un hueco y el siguiente va texto del cuerpo, donde `\{` es una llave escrita a propósito.
  const literal = (text: string) => text.replace(/\\([{}])/g, "$1");

  let out = "";
  let last = 0;
  for (const match of body.matchAll(HOLE)) {
    out += literal(body.slice(last, match.index));
    last = match.index + match[0].length;
    const inner = match[0].slice(2, -1);

    if (isAuto(inner)) {
      out += autos[inner];
      continue;
    }
    const variable = parseVariable(inner, now);
    if (variable) {
      const declared = first.get(variable.name) ?? variable;
      out +=
        declared.kind === "table" || declared.kind === "column"
          ? declared.default || declared.name
          : declared.default;
      continue;
    }
    // Hueco de CodeMirror: `*` y un nombre valen ellos mismos; `1:texto` vale el texto; `1` y vacío, nada.
    const numbered = /^\d+(?::(.*))?$/s.exec(inner);
    out += numbered ? (numbered[1] ?? "") : inner;
  }
  return out + literal(body.slice(last));
}
