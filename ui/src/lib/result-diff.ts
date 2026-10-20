/**
 * Comparar dos resultados de consulta, celda por celda.
 *
 * Es lógica pura y no un `{#each}` con comparaciones adentro porque lo que importa —qué fila se
 * empareja con cuál— es justo lo que se rompe callado: un emparejamiento por posición marca todo
 * como distinto en cuanto una fila se inserta arriba, y uno por clave tiene que decidir qué hacer
 * con una clave repetida. Las dos cosas se prueban sin servidor.
 *
 * Se comparan solo las columnas que tienen **el mismo nombre en los dos lados**; las que están en
 * uno solo se informan aparte, porque compararlas contra nada sería marcar toda la columna.
 * `null` (NULL de la base) y la cadena vacía son valores distintos, igual que en el resto de la
 * interfaz.
 */

export interface ResultData {
  columns: string[];
  rows: (string | null)[][];
}

export type RowStatus = "same" | "changed" | "added" | "removed";

export interface DiffRow {
  status: RowStatus;
  /** Clave con la que se emparejó; `null` al emparejar por posición. */
  key: string | null;
  /** Valores de las columnas comunes (`DiffResult.columns`), o `null` si la fila no existe de ese lado. */
  left: (string | null)[] | null;
  right: (string | null)[] | null;
  /** Por columna común: si la celda difiere. Solo tiene `true` en filas `changed`. */
  changed: boolean[];
}

export interface DiffCounts {
  same: number;
  changed: number;
  added: number;
  removed: number;
  /** Celdas distintas en total, para el resumen. */
  cells: number;
}

export interface DiffResult {
  /** Las columnas que están en los dos resultados, en el orden del izquierdo. */
  columns: string[];
  onlyLeft: string[];
  onlyRight: string[];
  rows: DiffRow[];
  counts: DiffCounts;
}

export type DiffOptions = {
  /** Columnas que identifican una fila; `null` empareja por posición. */
  key: string[] | null;
};

/** Qué columnas pueden servir de clave: las que existen de los dos lados. */
export function commonColumns(a: ResultData, b: ResultData): string[] {
  const right = new Set(b.columns);
  const seen = new Set<string>();
  const out: string[] = [];
  for (const name of a.columns) {
    if (right.has(name) && !seen.has(name)) {
      seen.add(name);
      out.push(name);
    }
  }
  return out;
}

/** Solo arma la clave interna; no se muestra. */
const SEP = "\u0000";

function keyOf(row: (string | null)[], indexes: number[]): string {
  // `null` y «» no pueden colapsar en la misma clave: son filas distintas.
  return indexes.map((i) => (row[i] === null || row[i] === undefined ? "\u0001" : `v${row[i]}`)).join(SEP);
}

function same(a: string | null | undefined, b: string | null | undefined): boolean {
  return (a ?? null) === (b ?? null);
}

export function diffResults(a: ResultData, b: ResultData, options: DiffOptions): DiffResult {
  const columns = commonColumns(a, b);
  const onlyLeft = a.columns.filter((name) => !b.columns.includes(name));
  const onlyRight = b.columns.filter((name) => !a.columns.includes(name));

  // Si un nombre se repite en un lado (SELECT a, a), vale la primera aparición.
  const leftIdx = columns.map((name) => a.columns.indexOf(name));
  const rightIdx = columns.map((name) => b.columns.indexOf(name));
  const project = (row: (string | null)[], idx: number[]) => idx.map((i) => row[i] ?? null);

  const counts: DiffCounts = { same: 0, changed: 0, added: 0, removed: 0, cells: 0 };
  const rows: DiffRow[] = [];

  const push = (
    key: string | null,
    left: (string | null)[] | null,
    right: (string | null)[] | null,
  ) => {
    let status: RowStatus;
    let changed = columns.map(() => false);
    if (left && right) {
      changed = columns.map((_, i) => !same(left[i], right[i]));
      const n = changed.filter(Boolean).length;
      status = n > 0 ? "changed" : "same";
      counts.cells += n;
    } else {
      status = left ? "removed" : "added";
    }
    counts[status] += 1;
    rows.push({ status, key, left, right, changed });
  };

  if (options.key === null) {
    const total = Math.max(a.rows.length, b.rows.length);
    for (let i = 0; i < total; i++) {
      push(
        null,
        i < a.rows.length ? project(a.rows[i], leftIdx) : null,
        i < b.rows.length ? project(b.rows[i], rightIdx) : null,
      );
    }
    return { columns, onlyLeft, onlyRight, rows, counts };
  }

  const keyCols = options.key;
  if (keyCols.length === 0 || keyCols.some((name) => !columns.includes(name))) {
    throw new Error("La clave tiene que ser una columna que esté en los dos resultados.");
  }
  const kl = keyCols.map((name) => a.columns.indexOf(name));
  const kr = keyCols.map((name) => b.columns.indexOf(name));

  // Una clave repetida se empareja por orden de aparición (la primera con la primera, …): descartar
  // las repetidas escondería filas, y marcar todas contra todas inventaría diferencias.
  const pending = new Map<string, (string | null)[][]>();
  for (const row of b.rows) {
    const k = keyOf(row, kr);
    const list = pending.get(k);
    if (list) list.push(row);
    else pending.set(k, [row]);
  }

  const shown = (row: (string | null)[], idx: number[]) =>
    idx.map((i) => row[i] ?? "NULL").join(", ");

  const matched = new Set<(string | null)[]>();
  for (const row of a.rows) {
    const match = pending.get(keyOf(row, kl))?.shift();
    if (match) matched.add(match);
    push(shown(row, kl), project(row, leftIdx), match ? project(match, rightIdx) : null);
  }
  // Lo que quedó sin pareja del derecho, en el orden en que venía.
  for (const row of b.rows) {
    if (!matched.has(row)) push(shown(row, kr), null, project(row, rightIdx));
  }

  return { columns, onlyLeft, onlyRight, rows, counts };
}

/** Un resumen de una línea para el encabezado del diálogo. */
export function diffSummary(result: DiffResult): string {
  const { same: s, changed, added, removed, cells } = result.counts;
  const columnsDiffer = result.onlyLeft.length + result.onlyRight.length > 0;
  if (changed === 0 && added === 0 && removed === 0 && !columnsDiffer) {
    return `Idénticos: ${s} ${s === 1 ? "fila" : "filas"} sin diferencias.`;
  }
  return [
    `${changed} ${changed === 1 ? "fila cambiada" : "filas cambiadas"} (${cells} ${cells === 1 ? "celda" : "celdas"})`,
    `${added} ${added === 1 ? "agregada" : "agregadas"}`,
    `${removed} ${removed === 1 ? "quitada" : "quitadas"}`,
    `${s} ${s === 1 ? "igual" : "iguales"}`,
  ].join(" · ");
}
