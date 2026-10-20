/**
 * Variables de una abreviatura: `${nombre:valor por omisión}`.
 *
 * Conviven con los huecos de CodeMirror sin pisarlos, y la diferencia es el `:`. `${tabla}` (sin dos
 * puntos) sigue siendo un hueco por el que se salta con el tabulador, y `${1:algo}` (número) sigue
 * siendo un hueco numerado con texto; lo que lleva un **nombre** y un valor por omisión es una
 * variable que se pregunta en un diálogo antes de insertar. Así las abreviaturas que ya existen no
 * cambian de comportamiento.
 *
 * Todo es texto y puro: el diálogo y el editor solo lo usan.
 */

const VARIABLE = /\$\{([\p{L}_][\p{L}\p{N}_]*):([^{}]*)\}/gu;

export interface SnippetVariable {
  name: string;
  /** El valor con el que arranca el campo del diálogo. */
  default: string;
}

/**
 * Las variables del cuerpo, sin repetir y en el orden en que aparecen. Si una se repite, vale el
 * valor por omisión de la primera: se pregunta una sola vez y rellena todas.
 */
export function variablesOf(body: string): SnippetVariable[] {
  const seen = new Map<string, SnippetVariable>();
  for (const match of body.matchAll(VARIABLE)) {
    if (!seen.has(match[1])) seen.set(match[1], { name: match[1], default: match[2] });
  }
  return [...seen.values()];
}

/**
 * El cuerpo con cada variable reemplazada por lo que se escribió (o su valor por omisión si falta).
 *
 * El valor va a pasar por el intérprete de plantillas de CodeMirror, que lee `${…}` como hueco: las
 * llaves se escapan con `\{` y `\}` —lo único que ese intérprete reconoce como escape— para que un
 * valor con llaves se inserte literal y no cree huecos.
 */
export function fillVariables(body: string, values: Record<string, string>): string {
  return body.replace(VARIABLE, (_, name: string, fallback: string) => {
    const value = values[name] ?? fallback;
    return value.replace(/[{}]/g, "\\$&");
  });
}
