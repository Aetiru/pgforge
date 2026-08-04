import { HighlightStyle } from "@codemirror/language";
import { tags } from "@lezer/highlight";

/**
 * El mapeo de token a color del SQL, compartido por el editor y las vistas de solo lectura del DDL.
 *
 * Los colores salen de las variables CSS `--cm-*` que declara cada tema, no de dos temas
 * conmutables: el resto de la aplicación resuelve el tema con el atributo `data-theme` del
 * documento, y un tema propio acá sería una segunda fuente de verdad que se desincroniza. Cambiar de
 * claro a oscuro repinta sin recrear nada, porque lo único que cambia son las variables.
 */
export const sqlHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--cm-keyword)", fontWeight: "500" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--cm-string)" },
  { tag: [tags.number, tags.bool, tags.null], color: "var(--cm-number)" },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    color: "var(--cm-comment)",
    fontStyle: "italic",
  },
  { tag: [tags.typeName, tags.typeOperator], color: "var(--cm-type)" },
  {
    tag: [tags.function(tags.variableName), tags.standard(tags.name)],
    color: "var(--cm-function)",
  },
  { tag: tags.operator, color: "var(--cm-operator)" },
]);
