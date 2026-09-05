/**
 * Rutas de scripts, armadas del lado de la interfaz: pura y sin tocar disco. El renombrado, la
 * carpeta nueva y el borrado de verdad los hace `pgforge_core::scripts` (comandos `script_rename` /
 * `script_create_folder` / `script_delete`), que ya pasan toda ruta por la guarda `within`. Acá solo
 * se decide **a qué ruta**, a partir de una que el árbol ya trajo del backend.
 */

/** Separador de la ruta: el que ya use `path`, para no mezclar `/` y `\` en el resultado. */
function separatorOf(path: string): "\\" | "/" {
  return path.includes("\\") ? "\\" : "/";
}

/**
 * Un nombre de archivo o carpeta, saneado lo mínimo indispensable: sin separadores de ruta —que un
 * título escrito a mano no tiene por qué traer, pero un renombrado descuidado podría interpretar
 * como una subcarpeta— y sin los espacios de los bordes.
 */
function sanitizeSegment(name: string): string {
  return name.trim().replaceAll("\\", "_").replaceAll("/", "_");
}

/** `oldPath` con su último tramo reemplazado por `name`, saneado. Mismo directorio, otro nombre. */
export function siblingPath(oldPath: string, name: string): string {
  const separator = separatorOf(oldPath);
  const at = oldPath.lastIndexOf(separator);
  const dir = at < 0 ? "" : oldPath.slice(0, at + 1);
  return `${dir}${sanitizeSegment(name)}`;
}

/** `parentPath` con `name` agregado como un tramo más: la ruta de una subcarpeta nueva. */
export function childPath(parentPath: string, name: string): string {
  const separator = separatorOf(parentPath);
  const sanitized = sanitizeSegment(name);
  return parentPath.endsWith(separator) ? `${parentPath}${sanitized}` : `${parentPath}${separator}${sanitized}`;
}

/**
 * La ruta nueva de `oldPath` al renombrarlo a `title`, en el mismo directorio, con `.sql` puesto si
 * el título no lo traía. `title` vacío o hecho solo de separadores queda como `.sql` — el llamador
 * valida antes de mandar algo así al servidor, esto no rechaza nada, solo arma la ruta.
 */
export function renamedScriptPath(oldPath: string, title: string): string {
  const sanitized = sanitizeSegment(title);
  const withExtension = sanitized.toLowerCase().endsWith(".sql") ? sanitized : `${sanitized}.sql`;
  return siblingPath(oldPath, withExtension);
}
