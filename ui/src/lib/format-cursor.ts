/**
 * Dónde cae la sentencia `index` dentro de un script ya formateado.
 *
 * `sql::format` (`pgforge_core::sql::format`) separa las sentencias con exactamente `";\n\n"`, así
 * que encontrar dónde empieza cada una no exige volver a partir el script del lado de la interfaz
 * —eso es justo lo que se quiso evitar al llevar el formateador al núcleo—. Fuera de rango, se cae
 * al principio o al final de la lista, nunca a una posición que no existe.
 */
export function offsetOfStatement(formatted: string, index: number): number {
  const parts = formatted.split(";\n\n");
  const clamped = Math.max(0, Math.min(index, parts.length - 1));

  let offset = 0;
  for (let i = 0; i < clamped; i++) {
    offset += parts[i].length + 3; // el ";\n\n" que separa de la siguiente
  }
  return offset;
}
