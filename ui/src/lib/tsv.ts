/**
 * Leer lo que Excel (u otra hoja) deja en el portapapeles: texto separado por tabuladores, una fila
 * por línea. Una celda con tabuladores, saltos de línea o comillas adentro viaja entre comillas y
 * con las comillas duplicadas, igual que un CSV. Es puro y con Vitest.
 */

/** Filas de celdas. Una línea final vacía —Excel siempre deja un salto al final— no es una fila. */
export function parseTsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  let atCellStart = true;

  const endCell = () => {
    row.push(cell);
    cell = "";
    atCellStart = true;
  };
  const endRow = () => {
    endCell();
    rows.push(row);
    row = [];
  };

  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          cell += '"';
          i++;
        } else quoted = false;
      } else cell += c;
    } else if (c === '"' && atCellStart) {
      quoted = true;
      atCellStart = false;
    } else if (c === "\t") endCell();
    else if (c === "\n") endRow();
    else if (c === "\r") {
      if (text[i + 1] === "\n") i++;
      endRow();
    } else {
      cell += c;
      atCellStart = false;
    }
  }
  // Lo que quedó sin cerrar es la última fila, salvo que el texto terminara justo en un salto.
  if (cell !== "" || row.length > 0 || quoted) endRow();
  return rows;
}

/** Cuántas filas y columnas pegar como máximo: más que eso es un error de pegado, no un uso. */
export const MAX_PASTE_ROWS = 5000;

export interface PastePlan {
  /** Por cada fila, índice de columna de la tabla → valor. */
  rows: Map<number, string | null>[];
  /** Si la primera fila se tomó como encabezados. */
  usedHeader: boolean;
  /** Columnas pegadas, en el orden en que llegan, por nombre. */
  columns: string[];
}

/**
 * Cómo se reparte lo pegado entre las columnas de la tabla.
 *
 * Si la primera fila trae solo nombres de columnas que existen, se toma como encabezado y el resto
 * se empareja **por nombre**; si no, se empareja por posición con las columnas escribibles, en su
 * orden. Una celda vacía es NULL: es lo que Excel entrega para una celda sin valor, y para una
 * cadena vacía real hay que editar la celda después.
 */
export function planPaste(
  grid: string[][],
  columns: readonly { name: string; generated: boolean }[],
  options: { header?: boolean } = {},
): PastePlan | string {
  if (grid.length === 0) return "No hay nada para pegar.";

  const names = columns.map((column) => column.name);
  const first = grid[0];
  const looksLikeHeader =
    first.length > 0 && first.every((cell) => names.includes(cell.trim())) && new Set(first).size === first.length;
  const useHeader = options.header ?? looksLikeHeader;

  let target: number[];
  if (useHeader) {
    target = first.map((cell) => names.indexOf(cell.trim()));
    if (target.some((index) => index === -1)) return "Algún encabezado no es una columna de la tabla.";
  } else {
    target = columns.map((column, index) => (column.generated ? -1 : index)).filter((index) => index >= 0);
  }
  if (target.some((index) => columns[index].generated)) {
    return "Las columnas generadas o de identidad las calcula el servidor: no se pueden pegar.";
  }

  const body = useHeader ? grid.slice(1) : grid;
  if (body.length === 0) return "Solo hay encabezados, sin filas.";
  if (body.length > MAX_PASTE_ROWS) return `Son ${body.length} filas; el máximo por pegado es ${MAX_PASTE_ROWS}.`;

  const rows = body.map((cells) => {
    const row = new Map<number, string | null>();
    target.forEach((columnIndex, position) => {
      const value = cells[position];
      if (value !== undefined) row.set(columnIndex, value === "" ? null : value);
    });
    return row;
  });

  return { rows, usedHeader: useHeader, columns: target.map((index) => names[index]) };
}
