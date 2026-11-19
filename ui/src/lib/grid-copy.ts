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

/**
 * El inverso de `delimited`: separa filas y celdas, deshaciendo el `quoted` de cada una. Sostiene lo
 * que copia cualquier planilla —comillas cuando el valor trae el separador, una comilla o un salto
 * de línea adentro— para que un bloque pegado entre igual que salió.
 */
export function parseDelimited(text: string, separator: "\t" | ","): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoting = false;

  for (let i = 0; i < text.length; i++) {
    const char = text[i];

    if (quoting) {
      if (char === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i += 1;
        } else {
          quoting = false;
        }
      } else {
        field += char;
      }
      continue;
    }

    if (char === '"' && field === "") {
      quoting = true;
    } else if (char === separator) {
      row.push(field);
      field = "";
    } else if (char === "\r") {
      // El `\n` que lo sigue ya cierra la fila; solo/suelto no significa nada.
    } else if (char === "\n") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else {
      field += char;
    }
  }
  row.push(field);
  rows.push(row);

  // Una fila final de una sola celda vacía es el salto de línea con el que termina el portapapeles,
  // no una fila pegada.
  const last = rows[rows.length - 1];
  if (rows.length > 1 && last.length === 1 && last[0] === "") rows.pop();

  return rows;
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

/**
 * La selección como tabla de Markdown, para pegar en un ticket o en un chat.
 *
 * `|` va escapado y los saltos de línea pasan a `<br>`: una celda con cualquiera de los dos rompería
 * la fila. NULL se escribe como `NULL` y no como celda vacía, que es otra cosa.
 */
export function asMarkdown(headers: string[], rows: (string | null)[][]): string {
  const cell = (value: string | null) =>
    value === null ? "NULL" : value.replace(/\|/g, "\\|").replace(/\r?\n/g, "<br>");
  const line = (cells: (string | null)[]) => `| ${cells.map(cell).join(" | ")} |`;
  return [
    line(headers),
    `| ${headers.map(() => "---").join(" | ")} |`,
    ...rows.map((row) => line(headers.map((_, index) => row[index] ?? null))),
  ].join("\n");
}

/** Cita un identificador solo cuando hace falta: minúsculas, dígitos y guion bajo se dejan tal cual. */
function ident(name: string): string {
  return /^[a-z_][a-z0-9_]*$/.test(name) ? name : `"${name.replace(/"/g, '""')}"`;
}

/** Un valor como literal de SQL: NULL, o texto entre comillas simples con las comillas duplicadas. */
function literal(value: string | null): string {
  if (value === null) return "NULL";
  return `'${value.replace(/'/g, "''")}'`;
}

/**
 * La selección como `INSERT`s, una sentencia por fila, para llevar filas a otra tabla o base.
 *
 * Todo viaja como texto entre comillas: PostgreSQL resuelve el tipo de un literal sin tipo contra la
 * columna, y así no hay que adivinar cuáles son números. `table` es lo que el usuario cambia después
 * de pegar cuando el resultado no sale de una sola tabla.
 */
export function asInserts(
  headers: string[],
  rows: (string | null)[][],
  table = "tabla",
): string {
  const columns = headers.map(ident).join(", ");
  return rows
    .map(
      (row) =>
        `INSERT INTO ${table} (${columns}) VALUES (${headers
          .map((_, index) => literal(row[index] ?? null))
          .join(", ")});`,
    )
    .join("\n");
}
