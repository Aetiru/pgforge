import { DEFAULT_FORMAT_OPTIONS, type FormatOptions } from "./ipc";

/**
 * Lo que se lee de `localStorage` es texto que alguien pudo editar a mano o que escribió una versión
 * vieja: se sanea campo por campo y lo que no es válido cae al valor de fábrica, en vez de tirar
 * todo el JSON o de mandarle al núcleo un valor que no entiende.
 */
export function sanitizeFormatOptions(raw: unknown): FormatOptions {
  const base = { ...DEFAULT_FORMAT_OPTIONS };
  if (typeof raw !== "object" || raw === null) return base;
  const value = raw as Record<string, unknown>;

  const keywordCase = ["upper", "lower", "preserve"].includes(value.keywordCase as string)
    ? (value.keywordCase as FormatOptions["keywordCase"])
    : base.keywordCase;
  const indent = ["2", "4", "tab"].includes(value.indent as string)
    ? (value.indent as FormatOptions["indent"])
    : base.indent;
  // `Number(null)` es 0 y `Number("")` también: sin exigir un número de verdad, un valor ausente se
  // leería como «cero líneas».
  const lines = value.linesBetweenQueries;
  const linesBetweenQueries =
    typeof lines === "number" && Number.isInteger(lines) && lines >= 0 && lines <= 3
      ? lines
      : base.linesBetweenQueries;
  const commaFirst = typeof value.commaFirst === "boolean" ? value.commaFirst : base.commaFirst;

  return { keywordCase, indent, linesBetweenQueries, commaFirst };
}

/** `null` (no hay nada guardado) o JSON roto dan los valores de fábrica. */
export function parseStoredFormatOptions(text: string | null): FormatOptions {
  if (text === null) return { ...DEFAULT_FORMAT_OPTIONS };
  try {
    return sanitizeFormatOptions(JSON.parse(text));
  } catch {
    return { ...DEFAULT_FORMAT_OPTIONS };
  }
}

export function isDefaultFormat(options: FormatOptions): boolean {
  return (Object.keys(DEFAULT_FORMAT_OPTIONS) as (keyof FormatOptions)[]).every(
    (key) => options[key] === DEFAULT_FORMAT_OPTIONS[key],
  );
}

/**
 * Una consulta con lo que cada opción cambia a la vista: dos sentencias (separación), una lista de
 * columnas (coma al principio), un `JOIN` y un `WHERE` (sangría) y palabras clave en el caso que se
 * elija. Escrita en minúsculas a propósito: así `preserve` se distingue de `upper`.
 */
export const FORMAT_SAMPLE =
  "select c.id, c.nombre, sum(p.total) as total from clientes c join pedidos p on p.cliente_id = c.id " +
  "where p.estado = 'pagado' and p.fecha >= now() - interval '30 days' group by c.id, c.nombre order by total desc;" +
  "update clientes set activo = false where id = 7;";
