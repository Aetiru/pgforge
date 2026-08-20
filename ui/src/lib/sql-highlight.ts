import { HighlightStyle } from "@codemirror/language";
import { tagHighlighter, tags, type Highlighter, type Tag } from "@lezer/highlight";

/**
 * El mapeo de token a color del SQL, compartido por el editor y las vistas de solo lectura del DDL.
 *
 * Los colores salen de las variables CSS `--cm-*` que declara cada tema, no de dos temas
 * conmutables: el resto de la aplicación resuelve el tema con el atributo `data-theme` del
 * documento, y un tema propio acá sería una segunda fuente de verdad que se desincroniza. Cambiar de
 * claro a oscuro repinta sin recrear nada, porque lo único que cambia son las variables.
 */
const STYLES: { tag: Tag | readonly Tag[]; css: Record<string, string> }[] = [
  { tag: tags.keyword, css: { color: "var(--cm-keyword)", "font-weight": "500" } },
  { tag: [tags.string, tags.special(tags.string)], css: { color: "var(--cm-string)" } },
  { tag: [tags.number, tags.bool, tags.null], css: { color: "var(--cm-number)" } },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment],
    css: { color: "var(--cm-comment)", "font-style": "italic" },
  },
  { tag: [tags.typeName, tags.typeOperator], css: { color: "var(--cm-type)" } },
  {
    tag: [tags.function(tags.variableName), tags.standard(tags.name)],
    css: { color: "var(--cm-function)" },
  },
  { tag: tags.operator, css: { color: "var(--cm-operator)" } },
];

/** Camel no: `HighlightStyle` acepta las propiedades tal cual se escriben en CSS. */
export const sqlHighlight = HighlightStyle.define(
  STYLES.map(({ tag, css }) => ({ tag, ...css })),
);

/**
 * El mismo mapeo, pero devolviendo un índice en vez de un color, para el SQL de adentro de un
 * `$$ … $$` (ver [`sql-nested`]).
 *
 * Ahí no alcanza con una clase: el cuerpo de la función es, para `lang-sql`, **una cadena**, así que
 * cada trozo termina con la clase de la cadena y la del token encima. Entre dos clases de la misma
 * especificidad gana la que el navegador leyó última, que no es algo que uno elija; el estilo en
 * línea, sí. De ahí que el resaltado anidado se aplique con `style` y no con `class`.
 */
export const sqlInlineHighlighter: Highlighter = tagHighlighter(
  STYLES.map(({ tag }, index) => ({ tag, class: `t${index}` })),
);

/** El `style` que le corresponde a lo que devolvió [`sqlInlineHighlighter`]; vacío si no hay. */
export function sqlInlineStyle(classes: string): string {
  return classes
    .split(" ")
    .map((name) => STYLES[Number(name.slice(1))]?.css)
    .filter((css): css is Record<string, string> => css !== undefined)
    .flatMap((css) => Object.entries(css).map(([property, value]) => `${property}: ${value}`))
    .join("; ");
}
