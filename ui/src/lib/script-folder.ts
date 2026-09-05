/**
 * Espejo en TypeScript de `pgforge_core::scripts::folder_name`, para una sola pregunta: **a qué
 * conexión pertenece** una carpeta de primer nivel del árbol de scripts.
 *
 * `ScriptFolder` no trae el `profileId` —el núcleo solo conoce el nombre de carpeta ya saneado
 * (`scripts::tree`, que lee el disco, no `connections.json`)—, así que la interfaz necesita poder ir
 * del nombre de la conexión al de su carpeta para saber contra quién abrir un script.
 *
 * Es la **única** excepción a la regla de «no duplicar `folder_name` en TypeScript»: esa regla es
 * sobre construir una ruta que se manda a escribir o borrar del lado de Rust —ahí toda ruta que
 * llega a un comando ya viene armada por `script_new_name`/`scripts_import_folder`—, y esto es
 * exactamente lo contrario, una comparación de solo lectura para decidir qué mostrar. Si no
 * coincide con ninguna carpeta —un nombre con caracteres raros, una conexión borrada— la carpeta
 * simplemente queda sin conexión reconocida, nunca se arma una ruta con esto.
 */

const INVALID_CHARS = new Set(["\\", "/", ":", "*", "?", '"', "<", ">", "|"]);

const RESERVED_NAMES = new Set([
  "CON",
  "PRN",
  "AUX",
  "NUL",
  "COM1",
  "COM2",
  "COM3",
  "COM4",
  "COM5",
  "COM6",
  "COM7",
  "COM8",
  "COM9",
  "LPT1",
  "LPT2",
  "LPT3",
  "LPT4",
  "LPT5",
  "LPT6",
  "LPT7",
  "LPT8",
  "LPT9",
]);

/** El nombre de carpeta que le correspondería a una conexión con este nombre. */
export function scriptFolderName(connection: string): string {
  let name = [...connection.trim()].map((char) => (INVALID_CHARS.has(char) ? "_" : char)).join("");

  while (name.endsWith(".") || name.endsWith(" ")) {
    name = name.slice(0, -1);
  }

  if (RESERVED_NAMES.has(name.toUpperCase())) name += "_";
  if (name === "") name = "_";

  return name;
}
