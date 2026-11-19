/**
 * La parte pura de recordar las pestañas: la forma de lo guardado y su lectura defensiva. Vive
 * aparte de `session-restore.svelte.ts` porque ese arrastra la conexión con el servidor y estas dos
 * funciones se prueban sin ella.
 */
export interface SavedTab {
  profileId: string;
  database: string;
  title: string;
  sql: string;
  scriptPath: string | null;
  filePath: string | null;
  savedId: number | null;
  savedName: string | null;
}

/** Techos para que un archivo enorme pegado en una pestaña no llene el almacenamiento del navegador. */
export const MAX_TABS = 40;
const MAX_SQL = 200_000;

export function snapshotOf(tab: {
  profileId: string;
  database: string;
  title: string;
  sql: string;
  scriptPath: string | null;
  filePath: string | null;
  savedId: number | null;
  savedName: string | null;
}): SavedTab {
  return {
    profileId: tab.profileId,
    database: tab.database,
    title: tab.title,
    sql: tab.sql.slice(0, MAX_SQL),
    scriptPath: tab.scriptPath,
    filePath: tab.filePath,
    savedId: tab.savedId,
    savedName: tab.savedName,
  };
}

/** Lo guardado con forma válida; todo lo demás se descarta, que el almacenamiento no es de fiar. */
export function parseSaved(raw: string | null): SavedTab[] {
  if (!raw) return [];
  try {
    const data: unknown = JSON.parse(raw);
    if (!Array.isArray(data)) return [];
    return data
      .filter(
        (item): item is SavedTab =>
          typeof item === "object" &&
          item !== null &&
          typeof item.profileId === "string" &&
          typeof item.database === "string" &&
          typeof item.title === "string" &&
          typeof item.sql === "string",
      )
      .map((item) => ({
        profileId: item.profileId,
        database: item.database,
        title: item.title,
        sql: item.sql,
        scriptPath: typeof item.scriptPath === "string" ? item.scriptPath : null,
        filePath: typeof item.filePath === "string" ? item.filePath : null,
        savedId: typeof item.savedId === "number" ? item.savedId : null,
        savedName: typeof item.savedName === "string" ? item.savedName : null,
      }))
      .slice(0, MAX_TABS);
  } catch {
    return [];
  }
}

