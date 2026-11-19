/**
 * La pestaña de una base: lo que tiene por dentro, como se abre una tabla para ver sus datos.
 *
 * Es de solo lectura y no toma sesión del lado de Rust —lee por una conexión del pool de esa base—,
 * así que `dispose()` queda como está en `Tab`. Se abre una pestaña por base: abrirla de nuevo trae
 * la que ya está en vez de duplicarla.
 */

import { Tab, tabs } from "./tabs.svelte";
import { databaseOverview, describeError, type DatabaseOverview } from "./ipc";

export class DatabaseTab extends Tab {
  readonly kind = "database" as const;

  overview = $state.raw<DatabaseOverview | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  constructor(profileId: string, database: string) {
    super(profileId, database, database);
  }

  async load() {
    this.loading = true;
    this.error = null;
    try {
      this.overview = await databaseOverview(this.profileId, this.database);
    } catch (error) {
      this.error = describeError(error);
    } finally {
      this.loading = false;
    }
  }
}

/** Abre —o trae al frente— la pestaña de la base y la lee. */
export async function openDatabase(profileId: string, database: string): Promise<DatabaseTab> {
  const existing = tabs.all.find(
    (tab): tab is DatabaseTab =>
      tab instanceof DatabaseTab && tab.profileId === profileId && tab.database === database,
  );
  if (existing) {
    tabs.activate(existing.key);
    await existing.load();
    return existing;
  }
  const tab = tabs.add(new DatabaseTab(profileId, database));
  await tab.load();
  return tab;
}
