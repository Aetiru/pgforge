import { invoke } from "./core";

/**
 * Una abreviatura del editor y el texto en el que se expande.
 *
 * Espejo de `sql::snippet::Snippet`. El `id` es estable para poder cambiarle la abreviatura sin que
 * se confunda con crear otra; la abreviatura es única sin distinguir mayúsculas.
 */
export interface Snippet {
  /** Ausente al crear una nueva: Rust le asigna uno. Presente siempre en lo que devuelve el servidor. */
  id?: string;
  /** Lo que se escribe antes del tabulador. */
  abbreviation: string;
  /** El texto que la reemplaza. Los `${}` son huecos por los que se salta con el tabulador. */
  body: string;
  /** Para qué sirve; es lo que la lista de sugerencias muestra al costado. */
  description: string;
  /** Para agrupar la lista; vacía = sin categoría (máx. 40 caracteres). */
  category: string;
  /** Dónde vale. Vacío = en todos lados. */
  scope: SnippetScope;
  /** Combinación que la expande sin escribir su nombre, al estilo CodeMirror (`Mod-Alt-1`). */
  shortcut: string | null;
}

/**
 * En qué servidores y bases vale una abreviatura. Las dos condiciones se suman (AND): con perfiles
 * y bases a la vez hay que estar en uno de esos servidores y en una de esas bases.
 */
export interface SnippetScope {
  profileIds: string[];
  databases: string[];
}

/** Cuántas veces se usó una abreviatura. Vive en un archivo aparte: es del equipo, no se exporta. */
export interface SnippetUse {
  uses: number;
  /** Segundos desde el epoch. */
  lastUsedAt: number | null;
}

/** Qué hacer con una importada cuya abreviatura ya existe. */
export type ImportMode = "skip" | "replace";

export interface ImportReport {
  /** La lista entera después de importar: la interfaz la toma tal cual. */
  snippets: Snippet[];
  added: number;
  replaced: number;
  /** Lo que no entró y por qué, ya escrito para mostrar. */
  skipped: string[];
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

/** Cuántas veces se usó cada abreviatura, por identificador. */
export const snippetUsage = () => invoke<Record<string, SnippetUse>>("snippet_usage");

/** Anota una expansión. Es un contador: si no se pudo escribir no falla. */
export const snippetUsed = (snippetId: string) => invoke<void>("snippet_used", { snippetId });

/**
 * Escribe las abreviaturas elegidas (o todas, con `null`) en un archivo de `pickSave`. No lleva
 * identificadores ni los servidores del alcance —son de esta máquina— ni el contador de usos.
 */
export const snippetsExport = (path: string, snippetIds: string[] | null) =>
  invoke<number>("snippets_export", { path, snippetIds });

/** Importa un archivo de `pickOpen`. Lo inválido se omite y se informa en `skipped`. */
export const snippetsImport = (path: string, mode: ImportMode) =>
  invoke<ImportReport>("snippets_import", { path, mode });
