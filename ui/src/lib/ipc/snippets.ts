import { invoke } from "./core";

/**
 * Una abreviatura del editor y el texto en el que se expande.
 *
 * Espejo de `sql::snippet::Snippet`. El `id` es estable para poder cambiarle la abreviatura sin que
 * se confunda con crear otra; la abreviatura es única sin distinguir mayúsculas.
 */
export interface Snippet {
  id: string;
  /** Lo que se escribe antes del tabulador. */
  abbreviation: string;
  /** El texto que la reemplaza. Los `${}` son huecos por los que se salta con el tabulador. */
  body: string;
  /** Para qué sirve; es lo que la lista de sugerencias muestra al costado. */
  description: string;
}

export const snippetsList = () => invoke<Snippet[]>("snippets_list");

/**
 * Guarda una nueva o reescribe una existente.
 *
 * Los tres devuelven la lista entera en vez de la fila tocada: es corta y así la interfaz no la
 * recompone a mano ni tiene que volver a pedirla.
 */
export const snippetSave = (snippet: Snippet) => invoke<Snippet[]>("snippet_save", { snippet });

export const snippetDelete = (snippetId: string) =>
  invoke<Snippet[]>("snippet_delete", { snippetId });

export const snippetsReset = () => invoke<Snippet[]>("snippets_reset");
