import { outline } from "../sql-outline";

export type RangeKind = "until" | "from";

/**
 * Qué texto corre «ejecutar hasta el cursor» o «desde el cursor», y dónde empieza dentro del
 * documento. Mismo recorte que `rangeUntil`/`rangeFrom` de `sql-outline.ts`, pero devolviendo el
 * desplazamiento: sin él, la marca de un error del servidor caería corrida (ver `QueryTab.run`,
 * donde `base` está en caracteres Unicode y no en unidades UTF-16 de CodeMirror).
 *
 * «Hasta» incluye la sentencia donde está el cursor; «desde» empieza en el inicio de esa. Con el
 * cursor entre dos sentencias «hasta» toma la anterior y «desde» la siguiente.
 */
export function rangeToRun(
  text: string,
  cursor: number,
  kind: RangeKind,
): { text: string; base: number } | null {
  const items = outline(text);
  const containing = items.find((item) => cursor >= item.from && cursor <= item.to);
  const target =
    kind === "until"
      ? (containing ?? [...items].reverse().find((item) => item.to < cursor))
      : (containing ?? items.find((item) => item.from > cursor));
  if (!target) return null;

  const raw = kind === "until" ? text.slice(0, target.to) : text.slice(target.from);
  const start = kind === "until" ? 0 : target.from;
  const lead = raw.length - raw.trimStart().length;
  const sliced = raw.trim();
  if (sliced === "") return null;
  return { text: sliced, base: [...text.slice(0, start + lead)].length };
}
