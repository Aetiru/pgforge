/**
 * Qué guías de sangría dibuja el árbol.
 *
 * Una línea por nivel en cada fila deja cinco rayas verticales fijas en un árbol de cinco niveles:
 * ruido permanente por un dato que se consulta de a ratos —de qué esquema cuelga esta tabla—. Acá se
 * calculan solo las guías de la fila elegida: la cadena de sus ancestros y hasta dónde llega el
 * bloque de cada uno, que es exactamente el camino que hay que seguir con la vista.
 *
 * Recibe los niveles y no las filas para poder probarse sin armar un árbol.
 */
export interface GuideSpan {
  /** Nivel del ancestro: la línea se dibuja en la columna de su chevron. */
  level: number;
  /** Primera y última fila que la línea atraviesa, las dos incluidas. */
  from: number;
  to: number;
}

export function guideSpans(levels: number[], selected: number): GuideSpan[] {
  if (selected < 0 || selected >= levels.length) return [];

  const spans: GuideSpan[] = [];
  // Se sube buscando el nivel de arriba por vez: la primera fila anterior que está en ese nivel es
  // el ancestro, porque la lista viene aplanada con el padre siempre antes que sus hijos.
  let want = levels[selected] - 1;
  for (let index = selected - 1; index >= 0 && want >= 0; index--) {
    if (levels[index] !== want) continue;

    // El bloque termina en la última fila seguida que cuelga del ancestro.
    let to = index;
    while (to + 1 < levels.length && levels[to + 1] > want) to++;

    spans.push({ level: want, from: index + 1, to });
    want--;
  }
  return spans;
}

/** Si la fila `row` lleva la guía del nivel `level`. */
export function guideAt(spans: GuideSpan[], row: number, level: number): boolean {
  return spans.some((span) => span.level === level && row >= span.from && row <= span.to);
}
