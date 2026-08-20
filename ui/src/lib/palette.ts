/**
 * Qué se ofrece al escribir en la paleta de comandos, y en qué orden.
 *
 * La parte que se puede equivocar no es abrir la ventanita: es que escribir «monit» encuentre
 * «Monitoreo», que «pedidos» encuentre la tabla antes que el servidor que se llama parecido, y que
 * con la caja vacía se vea lo de siempre y no una lista al azar. Eso es esto, y es puro.
 *
 * La coincidencia es por subsecuencia y no por substring: en una lista donde conviven acciones,
 * servidores, pestañas y tablas, uno escribe las iniciales («nc» para «Nueva consulta») tanto como
 * el pedazo del medio de un nombre.
 */

/** Qué es cada cosa que se ofrece, para agrupar y para desempatar. */
export type CommandGroup = "acción" | "vista" | "servidor" | "pestaña" | "objeto";

export interface Command {
  id: string;
  label: string;
  /** Lo que va a la derecha, en gris: la base, el atajo, de qué servidor es. */
  hint?: string;
  group: CommandGroup;
  run: () => void;
}

/**
 * Sin acentos y en minúsculas.
 *
 * Nadie escribe «configuración» con tilde en una caja de búsqueda, y sin esto «configuracion» no
 * encuentra nada.
 */
export function fold(text: string): string {
  return text
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();
}

/**
 * Qué tan bien coincide `query` con `text`, o `null` si no coincide.
 *
 * Más chico es mejor. Suma la distancia que hubo que saltar entre letra y letra, así una coincidencia
 * seguida («pedi» en «pedidos») gana sobre una desparramada («pedi» en «panel de indices»), y penaliza
 * arrancar tarde, para que lo que empieza con lo escrito quede primero.
 */
export function score(query: string, text: string): number | null {
  const needle = fold(query.trim());
  if (needle === "") return 0;

  const haystack = fold(text);
  let at = 0;
  let cost = 0;
  let previous = -1;

  for (const letter of needle) {
    const found = haystack.indexOf(letter, at);
    if (found === -1) return null;
    // El primer salto se cobra doble: es la diferencia entre «empieza con» y «lo tiene adentro».
    cost += previous === -1 ? found * 2 : found - previous - 1;
    previous = found;
    at = found + 1;
  }

  // A igualdad de saltos, gana el nombre más corto: es el más específico.
  return cost + haystack.length / 100;
}

/** El orden en que se muestran los grupos cuando todo lo demás empata. */
const ORDER: Record<CommandGroup, number> = {
  acción: 0,
  vista: 1,
  pestaña: 2,
  servidor: 3,
  objeto: 4,
};

/**
 * Lo que hay que mostrar para lo escrito.
 *
 * Con la caja vacía no se ordena nada: se devuelve la lista tal como la armó quien la ofrece, que ya
 * viene en el orden en que conviene verla.
 */
export function rank(commands: Command[], query: string, limit = 30): Command[] {
  if (query.trim() === "") return commands.slice(0, limit);

  const scored: { command: Command; score: number }[] = [];
  for (const command of commands) {
    // Se mira también la pista: «pedidos» tiene que encontrar «Datos de clientes · pedidos».
    const own = score(query, command.label);
    const withHint = command.hint ? score(query, `${command.label} ${command.hint}`) : null;
    const best = own ?? (withHint === null ? null : withHint + 1);
    if (best !== null) scored.push({ command, score: best });
  }

  scored.sort((a, b) => a.score - b.score || ORDER[a.command.group] - ORDER[b.command.group]);
  return scored.slice(0, limit).map((item) => item.command);
}
