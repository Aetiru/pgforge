/**
 * Cuánto mide una columna de la grilla antes de dibujarla.
 *
 * Las dos grillas que arman sus columnas en el momento —el resultado de una consulta y la pestaña
 * de datos— estimaban el ancho cada una con su propia copia de las mismas tres constantes, y por
 * eso las dos truncaban el encabezado igual: el avance por carácter estaba puesto en 7,3 píxeles,
 * medido a ojo, cuando la letra de la aplicación es Source Code Pro y avanza 0,6 em —8,4 píxeles
 * con el tamaño por omisión—. Con un carácter de menos por cada siete, «tipotrabajo» no entraba en
 * la columna que se calculó para él y aparecía cortado desde el primer dibujo, sin que hubiera nada
 * ancho que justificara el corte.
 *
 * Es una estimación y no una medición: `DataGrid` mide de verdad con un `canvas` cuando el usuario
 * pide ajustar una columna al contenido (doble clic en el borde), pero eso son una llamada y un
 * recorrido por columna, y acá se trata de elegir un ancho para veinte columnas antes del primer
 * cuadro.
 */

/**
 * Avance por carácter de Source Code Pro, en em. Es monoespaciada, así que un carácter mide siempre
 * lo mismo y alcanza con contar: no hace falta medir texto para saber cuánto va a ocupar.
 */
export const CHAR_EM = 0.6;

export interface WidthOptions {
  /** El texto más largo que tiene que entrar, en caracteres (encabezado incluido). */
  longest: number;
  /** Tamaño de letra de la grilla, en píxeles (`gridZoom.size`). */
  fontSize: number;
  min: number;
  max: number;
  /** Relleno de la celda más el aire que evita que el texto toque el borde. */
  padding: number;
}

/** El ancho de una columna, acotado entre su mínimo y su máximo. */
export function columnWidth({ longest, fontSize, min, max, padding }: WidthOptions): number {
  const scale = fontSize / 14;
  const text = Math.ceil(Math.max(0, longest) * CHAR_EM * fontSize);
  return Math.round(Math.min(max * scale, Math.max(min * scale, text + padding)));
}

/**
 * El ancho de la columna del número de fila. Sale de cuántas filas hay y no de una constante: con
 * mil filas sobran cuarenta píxeles fijos y con un millón faltan, y es una columna que se paga en
 * todas las demás —lo que ocupa el número es ancho que no ve el dato—.
 */
export function gutterWidth(rowCount: number, fontSize: number, padding = 16): number {
  const digits = Math.max(2, String(Math.max(1, rowCount)).length);
  return Math.round(digits * CHAR_EM * fontSize) + padding;
}
