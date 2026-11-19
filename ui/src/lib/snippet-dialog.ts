/**
 * Lo que el diálogo de abreviaturas decide y no es de ningún componente: nombres de copias, el texto
 * de cuántas veces se usó una, cómo se arma el alcance a partir del formulario, qué dice el
 * resumen de una importación y qué carpetas quedaron plegadas. Puro y con Vitest: el diálogo solo
 * pinta el resultado.
 */
import type { ImportReport, Snippet, SnippetScope } from "./ipc";
import { autoVariablesOf, variablesOf, type VariableKind } from "./snippet-vars";
import type { UsageInfo } from "./snippet-list";

/**
 * Una abreviatura libre para una copia: `sf_copia`, `sf_copia2`… La abreviatura es única sin
 * distinguir mayúsculas y no admite espacios, de ahí el guion bajo y no un « (copia)».
 */
export function freeCopyName(abbreviation: string, taken: string[]): string {
  const used = new Set(taken.map((name) => name.trim().toLowerCase()));
  const base = `${abbreviation.trim()}_copia`;
  if (!used.has(base.toLowerCase())) return base;
  for (let n = 2; ; n += 1) {
    const candidate = `${base}${n}`;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
}

/** Una copia lista para editar: sin id (es otra abreviatura) y sin atajo (el atajo es único). */
export function duplicateOf(source: Snippet, taken: string[]): Snippet {
  return {
    abbreviation: freeCopyName(source.abbreviation, taken),
    body: source.body,
    description: source.description,
    category: source.category,
    scope: { profileIds: [...source.scope.profileIds], databases: [...source.scope.databases] },
    shortcut: null,
  };
}

export function emptySnippet(): Snippet {
  // Sin `id`: Rust le pone uno. Mandar cadena vacía falla el parseo de UUID en vez de tomarse
  // como ausente.
  return {
    abbreviation: "",
    body: "",
    description: "",
    category: "",
    scope: { profileIds: [], databases: [] },
    shortcut: null,
  };
}

/** Cuándo se usó por última vez, a la escala de días que alcanza para decidir si sirve. */
function lastUsed(lastUsedAt: number, nowSeconds: number): string {
  const seconds = Math.max(0, nowSeconds - lastUsedAt);
  if (seconds < 86_400) return "hoy";
  const days = Math.floor(seconds / 86_400);
  return days === 1 ? "ayer" : `hace ${days} días`;
}

/** «142 usos · hoy», o «sin usos» cuando nunca se expandió. */
export function usageText(info: UsageInfo | undefined, nowSeconds: number): string {
  if (!info || info.uses === 0) return "sin usos";
  const uses = `${info.uses.toLocaleString("es")} ${info.uses === 1 ? "uso" : "usos"}`;
  return info.lastUsedAt === null ? uses : `${uses} · ${lastUsed(info.lastUsedAt, nowSeconds)}`;
}

/** Las bases escritas separadas por coma (o por salto de línea), sin vacíos ni repetidas. */
export function parseDatabases(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const part of text.split(/[,\n]/)) {
    const name = part.trim();
    if (name !== "" && !seen.has(name)) {
      seen.add(name);
      out.push(name);
    }
  }
  return out;
}

/** El estado del selector de alcance del formulario. */
export interface ScopeForm {
  /** `false` = «Todos los servidores»: el alcance queda vacío aunque haya cosas marcadas. */
  only: boolean;
  profileIds: string[];
  databasesText: string;
}

export function scopeToForm(scope: SnippetScope): ScopeForm {
  const only = scope.profileIds.length > 0 || scope.databases.length > 0;
  return { only, profileIds: [...scope.profileIds], databasesText: scope.databases.join(", ") };
}

/**
 * El alcance que se guarda. Con «Solo estos…» pero nada elegido queda vacío —vale en todos lados—:
 * una abreviatura que no vale en ninguna parte no se podría usar y parecería rota.
 */
export function formToScope(form: ScopeForm): SnippetScope {
  if (!form.only) return { profileIds: [], databases: [] };
  return { profileIds: [...new Set(form.profileIds)], databases: parseDatabases(form.databasesText) };
}

/** «Solo aquí»: el servidor y la base de la pestaña desde la que se abrió el diálogo. */
export function hereOnly(profileId: string | null, database: string | null): ScopeForm {
  return {
    only: true,
    profileIds: profileId ? [profileId] : [],
    databasesText: database ?? "",
  };
}

/** El resumen de una importación: «3 agregadas, 1 reemplazada, 2 omitidas». */
export function importSummary(report: Pick<ImportReport, "added" | "replaced" | "skipped">): string {
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const parts: string[] = [];
  parts.push(plural(report.added, "agregada", "agregadas"));
  if (report.replaced > 0) parts.push(plural(report.replaced, "reemplazada", "reemplazadas"));
  if (report.skipped.length > 0) parts.push(plural(report.skipped.length, "omitida", "omitidas"));
  return parts.join(", ");
}

/** Cómo se llama cada tipo de variable en el diálogo. */
export const KIND_LABEL: Record<VariableKind, string> = {
  text: "texto",
  list: "lista",
  date: "fecha",
  table: "tabla del catálogo",
  column: "columna del catálogo",
  number: "número",
};

export interface AskedVariable {
  name: string;
  kind: string;
  detail: string;
}

/** Las variables que preguntará al expandir, ya descritas para mostrarlas. */
export function askedVariables(body: string): AskedVariable[] {
  return variablesOf(body).map((variable) => ({
    name: variable.name,
    kind: KIND_LABEL[variable.kind],
    detail:
      variable.kind === "list"
        ? (variable.options ?? []).join(" · ")
        : variable.default === ""
          ? ""
          : `por omisión: ${variable.default}`,
  }));
}

/** Las que se llenan solas, en el orden en que aparecen. */
export function autoNames(body: string): string[] {
  return autoVariablesOf(body);
}

// --- Carpetas plegadas: preferencia local, como el resto de lo que recuerda la ventana. ---------

export const FOLDED_KEY = "pgforge.snippets.folded";

/** Lo que vuelve del almacenamiento se sanea: una lista de textos, nada más. */
export function parseFolded(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const value: unknown = JSON.parse(raw);
    return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

export function loadFolded(): Set<string> {
  try {
    return new Set(parseFolded(localStorage.getItem(FOLDED_KEY)));
  } catch {
    return new Set();
  }
}

export function saveFolded(folded: Set<string>): void {
  try {
    localStorage.setItem(FOLDED_KEY, JSON.stringify([...folded]));
  } catch {
    // Sin almacenamiento las carpetas vuelven a abrirse la próxima vez; no vale un aviso.
  }
}

/** Qué clase de `${…}` es un trozo del cuerpo, para pintarlo distinto. */
export type HoleKind = "hole" | "variable" | "list" | "typed" | "auto";

export function holeKind(hole: string): HoleKind {
  if (autoVariablesOf(hole).length > 0) return "auto";
  const variable = variablesOf(hole)[0];
  if (!variable) return "hole";
  if (variable.kind === "list") return "list";
  return variable.kind === "text" ? "variable" : "typed";
}

/** Los `${…}` de un texto, con su posición y su clase: lo que el editor del cuerpo resalta. */
export function findHoles(text: string): { from: number; to: number; kind: HoleKind }[] {
  const holes: { from: number; to: number; kind: HoleKind }[] = [];
  for (const match of text.matchAll(/\$\{[^{}]*\}/g)) {
    holes.push({ from: match.index, to: match.index + match[0].length, kind: holeKind(match[0]) });
  }
  return holes;
}
