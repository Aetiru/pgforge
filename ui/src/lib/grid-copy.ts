/**
 * Cómo se copia lo que está elegido en la grilla.
 *
 * Vive fuera del componente porque es lo único de la copia que puede estar mal de verdad: un valor
 * con una coma adentro, un salto de línea en un `text`, un NULL que no es lo mismo que una cadena
 * vacía. Todo eso se ve pegando en una planilla, que es tarde; acá se prueba sin montar nada.
 *
 * El NULL se copia como celda vacía: es lo que `COPY` de PostgreSQL entiende de vuelta, y «[null]»
 * —lo que muestra la pantalla— se pegaría como texto literal.
 */

/** Encierra el valor solo si el separador, una comilla o un salto aparecen adentro. */
export function quoted(value: string, separator: string): string {
  if (!value.includes(separator) && !value.includes('"') && !/[\r\n]/.test(value)) return value;
  return `"${value.replaceAll('"', '""')}"`;
}

/**
 * La selección como texto separado. Con tabulaciones es lo que espera una planilla al pegar; con
 * comas y encabezados es lo que se guarda en un archivo.
 */
export function delimited(
  headers: string[] | null,
  rows: (string | null)[][],
  separator: "\t" | ",",
): string {
  const lines: string[] = [];
  if (headers) lines.push(headers.map((header) => quoted(header, separator)).join(separator));
  for (const row of rows) {
    lines.push(row.map((value) => quoted(value ?? "", separator)).join(separator));
  }
  return lines.join("\n");
}

/** La selección como objetos, con el nombre de la columna por clave y el NULL conservado. */
export function asJson(headers: string[], rows: (string | null)[][]): string {
  const out = rows.map((row) => {
    const item: Record<string, string | null> = {};
    headers.forEach((header, index) => (item[header] = row[index] ?? null));
    return item;
  });
  return JSON.stringify(out, null, 2);
}

/**
 * Indenta el valor si es JSON, para el visor de celda. Un `jsonb` llega en una sola línea y leerlo
 * así es adivinarlo; lo que no sea JSON se devuelve tal cual, sin inventarle formato.
 */
export function pretty(value: string): string {
  const trimmed = value.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return value;
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    return value;
  }
}
