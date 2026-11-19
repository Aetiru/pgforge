import { isEmpty, noteKey, parseNotes, type NoteMap, type ObjectNote } from "./notes";

const KEY = "pgforge.notes";

/** Notas locales por objeto. Preferencia de esta computadora, como los marcadores. */
class Notes {
  map = $state.raw<NoteMap>(read());

  get(profileId: string, database: string, schema: string, name: string): ObjectNote | null {
    return this.map[noteKey(profileId, database, schema, name)] ?? null;
  }

  set(profileId: string, database: string, schema: string, name: string, note: ObjectNote) {
    const key = noteKey(profileId, database, schema, name);
    const next = { ...this.map };
    if (isEmpty(note)) delete next[key];
    else next[key] = note;
    this.map = next;
    try {
      localStorage.setItem(KEY, JSON.stringify(next));
    } catch {
      // Sin `localStorage` la nota vale hasta cerrar la ventana.
    }
  }
}

function read(): NoteMap {
  try {
    return parseNotes(localStorage.getItem(KEY));
  } catch {
    return {};
  }
}

export const notes = new Notes();
