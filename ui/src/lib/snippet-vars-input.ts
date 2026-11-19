import type { SnippetVariable } from "./snippet-vars";

/**
 * Qué hacer con lo que se escribió en el campo de una variable al apretar «Insertar».
 *
 * `value: null` significa «no mandar nada»: `fillVariables` usa entonces el valor por omisión. Es la
 * diferencia entre un selector de fecha que se vació a propósito (vale la fecha por omisión) y un
 * texto vacío (que sí es un valor). Puro y con Vitest: el diálogo solo pinta.
 */
export interface VariableInput {
  value: string | null;
  error: string | null;
}

const DATE = /^\d{4}-\d{2}-\d{2}$/;
const NUMBER = /^[+-]?\d+(?:[.,]\d+)?$/;

export function validateVariable(variable: SnippetVariable, raw: string): VariableInput {
  switch (variable.kind) {
    case "date": {
      const text = raw.trim();
      if (text === "") return { value: null, error: null };
      // `<input type="date">` ya entrega este formato; la comprobación cubre lo escrito a mano en
      // los navegadores que lo dejan escribir como texto.
      if (!DATE.test(text) || Number.isNaN(Date.parse(`${text}T00:00:00`))) {
        return { value: null, error: "Escribe una fecha válida (AAAA-MM-DD)." };
      }
      return { value: text, error: null };
    }
    case "number": {
      const text = raw.trim();
      if (text === "") return { value: null, error: null };
      if (!NUMBER.test(text)) return { value: null, error: "Tiene que ser un número." };
      // PostgreSQL no entiende la coma decimal: lo que se escribe como en español se inserta bien.
      return { value: text.replace(",", "."), error: null };
    }
    case "list":
      return { value: raw, error: null };
    case "table":
    case "column":
      // Los espacios de los costados vienen de pegar un nombre; adentro de un nombre no se tocan.
      return { value: raw.trim(), error: null };
    default:
      return { value: raw, error: null };
  }
}
