/** Dónde está el cursor, en lo que se muestra en la barra de estado. */
export interface CursorInfo {
  line: number;
  column: number;
  /** Cuántos caracteres hay seleccionados; 0 sin selección. */
  selected: number;
}

/**
 * Línea y columna (base 1) de `head` dentro de `text`, y el largo de la selección.
 *
 * Las posiciones de CodeMirror cuentan unidades UTF-16, igual que `String`, así que se puede
 * indexar el texto directo; la columna cuenta caracteres desde el último salto de línea.
 */
export function cursorInfo(text: string, anchor: number, head: number): CursorInfo {
  const at = Math.min(Math.max(head, 0), text.length);
  let line = 1;
  let lineStart = 0;
  for (let index = text.indexOf("\n"); index !== -1 && index < at; index = text.indexOf("\n", index + 1)) {
    line += 1;
    lineStart = index + 1;
  }
  return { line, column: at - lineStart + 1, selected: Math.abs(head - anchor) };
}
