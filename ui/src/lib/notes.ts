/**
 * Notas y etiquetas de los objetos de una base: «esta tabla la usa el módulo de facturación».
 *
 * Viven en esta computadora (no en la base: pgforge no escribe en servidores ajenos para guardar
 * apuntes) y se identifican por servidor, base, esquema y nombre, no por oid: un oid cambia al
 * recrear el objeto y la nota tiene que sobrevivir a eso. Es puro y con Vitest.
 */

export interface ObjectNote {
  note: string;
  tags: string[];
}

export type NoteMap = Record<string, ObjectNote>;

export const MAX_TAGS = 10;
export const MAX_TAG_LENGTH = 30;
export const MAX_NOTE_LENGTH = 2000;

/** La clave de un objeto. El separador `\u0001` no puede estar en un nombre que alguien escriba. */
export function noteKey(profileId: string, database: string, schema: string, name: string): string {
  return [profileId, database, schema, name].join("\u0001");
}

/** Etiquetas desde lo que se escribe: separadas por coma o espacio, sin `#`, sin repetir. */
export function normalizeTags(text: string): string[] {
  const seen = new Set<string>();
  const tags: string[] = [];
  for (const piece of text.split(/[\s,]+/)) {
    const tag = piece.replace(/^#+/, "").trim().toLowerCase().slice(0, MAX_TAG_LENGTH);
    if (tag === "" || seen.has(tag)) continue;
    seen.add(tag);
    tags.push(tag);
    if (tags.length === MAX_TAGS) break;
  }
  return tags;
}

/** Lo guardado en `localStorage`, saneado: lo que no tenga la forma esperada se descarta. */
export function parseNotes(raw: string | null): NoteMap {
  if (!raw) return {};
  try {
    const data: unknown = JSON.parse(raw);
    if (typeof data !== "object" || data === null || Array.isArray(data)) return {};
    const out: NoteMap = {};
    for (const [key, value] of Object.entries(data)) {
      if (typeof value !== "object" || value === null) continue;
      const { note, tags } = value as { note?: unknown; tags?: unknown };
      const cleanNote = typeof note === "string" ? note.slice(0, MAX_NOTE_LENGTH) : "";
      const cleanTags = Array.isArray(tags)
        ? normalizeTags(tags.filter((tag): tag is string => typeof tag === "string").join(" "))
        : [];
      if (cleanNote !== "" || cleanTags.length > 0) out[key] = { note: cleanNote, tags: cleanTags };
    }
    return out;
  } catch {
    return {};
  }
}

/** Una nota vacía y sin etiquetas no se guarda: dejaría una entrada que no dice nada. */
export function isEmpty(note: ObjectNote): boolean {
  return note.note.trim() === "" && note.tags.length === 0;
}

/**
 * Si una nota responde a lo que se busca. Con `#` al principio se busca por etiqueta (por prefijo:
 * `#fact` encuentra `#facturación`); sin él, por el texto de la nota. Sin distinguir mayúsculas.
 */
export function matchesNote(note: ObjectNote | null, needle: string): boolean {
  if (!note || needle === "") return false;
  const lower = needle.toLowerCase();
  if (lower.startsWith("#")) {
    const tag = lower.replace(/^#+/, "");
    return tag !== "" && note.tags.some((item) => item.startsWith(tag));
  }
  return note.note.toLowerCase().includes(lower);
}

/** Lo que se agrega a la pista de un objeto en la paleta para que se pueda encontrar por su nota. */
export function noteHint(note: ObjectNote | null): string {
  if (!note) return "";
  const tags = note.tags.map((tag) => `#${tag}`).join(" ");
  const text = note.note.trim().replace(/\s+/g, " ").slice(0, 60);
  return [tags, text].filter(Boolean).join(" · ");
}
