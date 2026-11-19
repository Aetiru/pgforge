/**
 * Separador de miles para los números de una grilla: `1234567.89` se ve `1.234.567,89`.
 *
 * Solo cambia lo que se ve; lo que se copia y lo que se edita sigue siendo el valor de la base. No
 * toca nada que no sea un número llano: un `00123` (un código, no una cantidad), un `1e5`, una
 * fecha o un texto pasan tal cual, así que activarlo sobre una columna mezclada no estropea nada.
 * Es puro y con Vitest.
 */

const PLAIN = /^(-?)(\d+)(?:\.(\d+))?$/;

/** Los miles y la coma decimal, como se escribe en español. */
export function formatThousands(text: string): string {
  const match = PLAIN.exec(text);
  if (!match) return text;

  const [, sign, integer, fraction] = match;
  // Un cero a la izquierda es parte del dato (códigos, folios): no se le ponen puntos.
  if (integer.length > 1 && integer.startsWith("0")) return text;
  if (integer.length <= 3 && fraction === undefined) return text;

  const grouped = integer.replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return fraction === undefined ? `${sign}${grouped}` : `${sign}${grouped},${fraction}`;
}
