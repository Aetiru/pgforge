/**
 * Cuántas filas trae cada tanda de la grilla de datos.
 *
 * La tabla nunca se lee entera: se pide una tanda y el scroll pide la siguiente al acercarse al
 * final. Cuál es el tamaño de esa tanda depende de para qué se abrió la tabla —mirar las últimas
 * altas o revisar diez mil filas— y de qué tan anchas son sus columnas, así que es del usuario y no
 * una constante escondida en el núcleo.
 *
 * Se guarda como preferencia y no por pestaña: quien lo sube una vez lo quiere para la próxima.
 */

const KEY = "pgforge.rows.page";

export const PAGE_SIZES = [100, 200, 500, 1000, 5000] as const;

/** Entran de sobra en una pantalla y la primera tanda llega enseguida. */
export const DEFAULT_PAGE_SIZE = 200;

/**
 * Hasta dónde sigue trayendo el scroll por su cuenta. Pasado ese techo hay que pedirlo con el botón:
 * cada tanda se suma a lo que ya está en memoria y una barra arrastrada sin querer no puede terminar
 * con la tabla entera adentro de la ventana.
 */
export const AUTO_LIMIT = 10_000;

function stored(): number {
  const value = Number(localStorage.getItem(KEY));
  return (PAGE_SIZES as readonly number[]).includes(value) ? value : DEFAULT_PAGE_SIZE;
}

class Paging {
  size = $state(stored());

  set(size: number) {
    this.size = size;
    localStorage.setItem(KEY, String(size));
  }
}

export const paging = new Paging();
