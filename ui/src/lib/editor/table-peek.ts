import type { Extension } from "@codemirror/state";
import { hoverTooltip } from "@codemirror/view";
import type { SchemaRelation } from "../ipc";
import { cteRelations, hoverInfo, qualifiedNameAt, relationAt, tablesInScope } from "../sql-complete";

/**
 * A qué tabla o vista apunta el identificador de `pos`, si la tarjeta de vista rápida corresponde.
 *
 * Calla ante lo que no es **seguro** una relación del catálogo: una columna (ya tiene su propio
 * globo), una CTE —puede llamarse igual que una tabla real y mostrar la tarjeta de la otra sería
 * mentir— y lo que no resuelve. Una tarjeta equivocada es peor que ninguna.
 */
export function peekTarget(
  text: string,
  pos: number,
  relations: SchemaRelation[],
): { relation: SchemaRelation; from: number; to: number } | null {
  const qualified = qualifiedNameAt(text, pos);
  if (!qualified) return null;

  const refs = tablesInScope(text, pos);
  if (hoverInfo(relations, refs, qualified)?.kind === "column") return null;

  if (
    qualified.schema === null &&
    cteRelations(text, pos, relations).some((cte) => cte.name === qualified.name)
  ) {
    return null;
  }

  const relation = relationAt(relations, refs, qualified);
  if (!relation || relation.oid < 0) return null;
  return { relation, from: qualified.from, to: qualified.to };
}

/** Tiempo con el cursor quieto antes de mostrar la tarjeta; el mismo que en el árbol. */
export const PEEK_DELAY_MS = 500;

/**
 * La misma tarjeta que el árbol (`ObjectPeek`) al pasar el cursor sobre una tabla del editor. Es un
 * `hoverTooltip` aparte del de columnas y funciones: cada uno responde solo a lo suyo y devuelve
 * `null` en lo demás, así no se pisan.
 *
 * Quién dibuja la tarjeta lo decide `mount`: devuelve cómo desmontarla. Acá no se importa Svelte ni
 * la tarjeta para que esto siga siendo del editor.
 */
export function tablePeek(options: {
  relations: () => SchemaRelation[];
  mount: (relation: SchemaRelation, target: HTMLElement) => () => void;
}): Extension {
  return hoverTooltip(
    (view, pos) => {
      const found = peekTarget(view.state.doc.toString(), pos, options.relations());
      if (!found) return null;
      return {
        pos: found.from,
        end: found.to,
        create: () => {
          const dom = document.createElement("div");
          const destroy = options.mount(found.relation, dom);
          return { dom, destroy };
        },
      };
    },
    { hoverTime: PEEK_DELAY_MS },
  );
}
