/**
 * Las pestañas del panel principal.
 *
 * Hay tres clases de pestaña —una consulta, los datos de una tabla y el diagrama de un esquema— y
 * conviven en la misma barra, así que la lista vive acá y no dentro de ninguna de ellas. Lo único
 * que comparten es qué son y contra qué base corren; todo lo demás lo pone cada una.
 */

let sequence = 0;

export type TabKind = "query" | "data" | "erd";

export abstract class Tab {
  /** Identificador local. Existe desde antes que cualquier conexión, que puede tardar o fallar. */
  readonly key = `tab-${++sequence}`;
  readonly profileId: string;
  readonly database: string;
  readonly title: string;

  abstract readonly kind: TabKind;

  constructor(profileId: string, database: string, title: string) {
    this.profileId = profileId;
    this.database = database;
    this.title = title;
  }

  /** Se llama al cerrar la pestaña. Por omisión no hay nada que soltar. */
  async dispose(): Promise<void> {}
}

class Tabs {
  all = $state<Tab[]>([]);
  /** `null` significa que se está mirando el detalle del objeto, no una pestaña. */
  active = $state<string | null>(null);

  get current(): Tab | null {
    return this.all.find((tab) => tab.key === this.active) ?? null;
  }

  /** Agrega la pestaña y la deja seleccionada. */
  add<T extends Tab>(tab: T): T {
    this.all.push(tab);
    this.active = tab.key;
    return tab;
  }

  async close(key: string) {
    const tab = this.all.find((item) => item.key === key);
    if (!tab) return;

    this.all = this.all.filter((item) => item.key !== key);
    if (this.active === key) {
      this.active = this.all.at(-1)?.key ?? null;
    }

    // Que falle soltar el recurso no puede impedir cerrar: la pestaña ya no está en pantalla.
    await tab.dispose().catch(() => {});
  }

  /** Cierra las pestañas de un servidor que se desconectó; su conexión ya no existe. */
  async closeFor(profileId: string) {
    for (const tab of this.all.filter((item) => item.profileId === profileId)) {
      await this.close(tab.key);
    }
  }
}

export const tabs = new Tabs();
