/**
 * Qué se hace con el texto de una sentencia de `pg_stat_statements` antes de explicarla.
 *
 * El texto viene normalizado: los valores son `$1`, `$2`… Con eso el servidor no planifica nada
 * (no sabe de qué tipo es cada uno ni cuánto vale, y el plan depende de los valores), así que una
 * sentencia con parámetros no se explica sola: se abre escrita, con un aviso arriba, para que quien la
 * mira ponga valores de verdad. La que no tiene parámetros se explica en el acto.
 */

export function needsValues(sql: string): boolean {
  return /\$\d+/.test(sql);
}

/** El aviso que va arriba de una sentencia con parámetros, con cuáles son. */
export function withValuesHint(sql: string): string {
  const params = [...new Set(sql.match(/\$\d+/g) ?? [])].sort(
    (a, b) => Number(a.slice(1)) - Number(b.slice(1)),
  );
  return `-- Esta sentencia viene de pg_stat_statements con los valores quitados (${params.join(", ")}).\n-- Reemplazalos por valores reales y apretá «Explicar»: el plan depende de ellos.\n${sql}`;
}
