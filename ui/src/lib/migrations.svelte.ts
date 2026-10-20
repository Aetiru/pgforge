/**
 * La pestaña de migraciones de un servidor y una base.
 *
 * Como la comparación, no toma ninguna sesión del lado de Rust: lee la carpeta y el registro y los
 * dibuja, así que `dispose()` queda como está en `Tab`. Lo que guarda es la carpeta elegida, que
 * solo vale mientras dure la autorización del diálogo (vive en la memoria de Rust): tras reiniciar
 * la aplicación hay que volver a elegirla, y lo único que se recuerda es dónde abrir el diálogo.
 */

import { Tab, tabs } from "./tabs.svelte";
import { describeError, migrationOverview, pickOpen, type MigrationOverview } from "./ipc";

const REMEMBER = "pgforge.migrations.folder";

function rememberKey(profileId: string, database: string) {
  return `${REMEMBER}.${profileId}.${database}`;
}

/** Dónde abrir el diálogo la próxima vez. Es una orientación: no autoriza nada. */
function remembered(profileId: string, database: string): string | undefined {
  try {
    return localStorage.getItem(rememberKey(profileId, database)) ?? undefined;
  } catch {
    return undefined;
  }
}

function remember(profileId: string, database: string, folder: string) {
  try {
    localStorage.setItem(rememberKey(profileId, database), folder);
  } catch {
    // Sin almacenamiento el diálogo abre donde quiera: no es un error que haya que mostrar.
  }
}

export class MigrationsTab extends Tab {
  readonly kind = "migrations" as const;

  folder = $state<string | null>(null);
  overview = $state.raw<MigrationOverview | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  constructor(profileId: string, database: string, folder: string | null) {
    super(profileId, database, `Migraciones · ${database}`);
    this.folder = folder;
  }

  async load() {
    if (!this.folder) return;
    this.loading = true;
    this.error = null;
    try {
      this.overview = await migrationOverview(this.profileId, this.database, this.folder);
    } catch (error) {
      this.error = describeError(error);
      this.overview = null;
    } finally {
      this.loading = false;
    }
  }

  /** Elige la carpeta con el diálogo nativo (que la deja autorizada) y la lee. */
  async chooseFolder() {
    const chosen = await pickOpen({
      directory: true,
      title: "Carpeta de migraciones",
      defaultPath: remembered(this.profileId, this.database),
    });
    if (!chosen) return;
    this.folder = chosen;
    remember(this.profileId, this.database, chosen);
    await this.load();
  }
}

/**
 * Abre (o trae al frente) las migraciones de una base. Con `folder` ya elegida —por ejemplo, la
 * que acaba de usar «Guardar como migración»— la lee de una vez.
 */
export async function openMigrations(
  profileId: string,
  database: string,
  folder: string | null = null,
): Promise<MigrationsTab> {
  const open = tabs.all.find(
    (tab): tab is MigrationsTab =>
      tab instanceof MigrationsTab && tab.profileId === profileId && tab.database === database,
  );
  const tab = open ?? tabs.add(new MigrationsTab(profileId, database, folder));
  tabs.activate(tab.key);
  if (folder) {
    tab.folder = folder;
    remember(profileId, database, folder);
  }
  await tab.load();
  return tab;
}
