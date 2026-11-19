/**
 * Mapa de rectángulos proporcionales (algoritmo «squarified», Bruls et al.): cada valor recibe un
 * rectángulo de área proporcional, lo más cuadrado posible, para que se puedan comparar a ojo.
 *
 * Puro y sin DOM: devuelve las posiciones y el componente solo dibuja. Los rectángulos salen **en el
 * orden de entrada** y no ordenados por tamaño, para que quien llama pueda asociarlos a sus datos
 * sin volver a buscarlos.
 */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function squarify(values: number[], box: Rect): Rect[] {
  const empty: Rect = { x: box.x, y: box.y, w: 0, h: 0 };
  const out: Rect[] = values.map(() => ({ ...empty }));
  const total = values.reduce((sum, v) => sum + Math.max(v, 0), 0);
  if (total <= 0 || box.w <= 0 || box.h <= 0) return out;

  // De mayor a menor: es lo que hace que las filas queden parejas.
  const order = values
    .map((value, index) => ({ value: Math.max(value, 0), index }))
    .filter((item) => item.value > 0)
    .sort((a, b) => b.value - a.value);
  const scale = (box.w * box.h) / total;
  const areas = order.map((item) => item.value * scale);

  let { x, y, w, h } = box;
  let start = 0;
  while (start < areas.length) {
    const side = Math.min(w, h);
    let end = start + 1;
    let row = [areas[start]];
    // Se suma a la fila mientras no empeore la peor proporción.
    while (end < areas.length && worst([...row, areas[end]], side) <= worst(row, side)) {
      row.push(areas[end]);
      end += 1;
    }
    const sum = row.reduce((a, b) => a + b, 0);
    if (w >= h) {
      // Franja vertical a la izquierda.
      const stripW = sum / h;
      let cy = y;
      row.forEach((area, i) => {
        const rh = area / stripW;
        out[order[start + i].index] = { x, y: cy, w: stripW, h: rh };
        cy += rh;
      });
      x += stripW;
      w -= stripW;
    } else {
      // Franja horizontal arriba.
      const stripH = sum / w;
      let cx = x;
      row.forEach((area, i) => {
        const rw = area / stripH;
        out[order[start + i].index] = { x: cx, y, w: rw, h: stripH };
        cx += rw;
      });
      y += stripH;
      h -= stripH;
    }
    start = end;
    row = [];
  }
  return out;
}

/** La peor proporción (lado largo sobre lado corto) de una fila de áreas contra el lado dado. */
function worst(row: number[], side: number): number {
  const sum = row.reduce((a, b) => a + b, 0);
  const max = Math.max(...row);
  const min = Math.min(...row);
  return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
}
