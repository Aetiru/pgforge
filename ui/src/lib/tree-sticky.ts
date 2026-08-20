/**
 * Qué rótulo queda anclado arriba del árbol mientras se desplaza.
 *
 * Un esquema con cuatrocientas tablas deja el rótulo «Tablas» —y el nombre del esquema, y el de la
 * base— fuera de pantalla a los pocos píxeles, así que a mitad de la lista no hay forma de saber
 * qué se está mirando sin volver arriba. Se ancla **uno solo**, el más cercano: apilar la cadena
 * entera se come el alto de la ventana, que es justo lo que hace falta para ver filas.
 */

/** Lo que hace falta saber de cada fila; el resto del `Row` no importa acá. */
export interface StickyRow {
  level: number;
  /** Si la fila agrupa en vez de nombrar algo: una carpeta del catálogo o una de conexiones. */
  isSection: boolean;
  /** Una carpeta de conexiones. Sus servidores están al mismo nivel que ella, no debajo. */
  isGroup: boolean;
}

/**
 * El índice del rótulo que corresponde anclar cuando la primera fila visible es `from`, o `null` si
 * esa fila no cuelga de ninguno.
 *
 * Se sube por la cadena de ancestros —cada vez que aparece un nivel menor— y se devuelve el primer
 * rótulo. Las carpetas de conexiones se reconocen aparte porque no sangran a sus servidores: por
 * nivel nunca serían ancestros de nadie.
 *
 * `cut` dice si esa primera fila está **cortada** por el borde de arriba. Importa cuando ella misma
 * es el rótulo: anclarla estando entera en su lugar dibuja una copia exactamente encima del
 * original, y como el ancla va por delante se come sus clics —la carpeta de arriba de todo no se
 * podía abrir ni renombrar, y sacarla dejaba muerta a la que quedaba primera—. Con la fila entera a
 * la vista no hay nada que anclar: ya se está viendo.
 */
export function stickyIndex(rows: StickyRow[], from: number, cut: boolean): number | null {
  if (from < 0 || from >= rows.length) return null;
  // La propia fila es el rótulo: se ancla solo mientras se está yendo por arriba.
  if (rows[from].isSection) return cut ? from : null;

  let level = rows[from].level;
  for (let index = from - 1; index >= 0; index--) {
    const row = rows[index];
    if (row.isGroup) return row.isSection ? index : null;
    if (row.level < level) {
      level = row.level;
      if (row.isSection) return index;
    }
  }
  return null;
}
