import type { AliasOccurrence } from "../sql-rename";

export interface PreviewLine {
  /** Número de línea, desde 1. */
  line: number;
  before: string;
  after: string;
}

/**
 * Las líneas que cambiarían, antes y después, para mostrarlas en el diálogo. Se aplica el cambio
 * sobre cada línea de forma independiente: lo que importa ver es **dónde** aparece el alias, y un
 * diff de la sentencia entera tapa eso con ruido.
 */
export function previewLines(
  text: string,
  occurrences: AliasOccurrence[],
  newName: string,
  max = 6,
): PreviewLine[] {
  const lineStarts = [0];
  for (let at = 0; at < text.length; at += 1) if (text.charCodeAt(at) === 10) lineStarts.push(at + 1);

  const byLine = new Map<number, AliasOccurrence[]>();
  for (const occurrence of occurrences) {
    let line = lineStarts.length - 1;
    while (lineStarts[line] > occurrence.from) line -= 1;
    byLine.set(line, [...(byLine.get(line) ?? []), occurrence]);
  }

  return [...byLine.entries()]
    .sort((a, b) => a[0] - b[0])
    .slice(0, max)
    .map(([line, list]) => {
      const start = lineStarts[line];
      const end = line + 1 < lineStarts.length ? lineStarts[line + 1] - 1 : text.length;
      const before = text.slice(start, end);
      let after = "";
      let at = 0;
      for (const o of [...list].sort((a, b) => a.from - b.from)) {
        after += before.slice(at, o.from - start) + newName;
        at = o.to - start;
      }
      after += before.slice(at);
      return { line: line + 1, before: before.trim(), after: after.trim() };
    });
}

/** Cuántas líneas distintas tocaría el cambio. */
export function linesTouched(text: string, occurrences: AliasOccurrence[]): number {
  return previewLines(text, occurrences, "", Number.MAX_SAFE_INTEGER).length;
}
