/**
 * Las pestañas del panel principal.
 *
 * Hay cuatro clases de pestaña —una consulta, los datos de una tabla, el diagrama de un esquema y
 * la comparación de dos— y conviven en la misma barra, así que la lista vive acá y no dentro de
 * ninguna de ellas. Lo único que comparten es qué son y contra qué base corren; todo lo demás lo
 * pone cada una.
 */

let sequence = 0;

export type TabKind = "query" | "data" | "erd" | "compare";

export abstract class Tab {
  /** Identificador local. Existe desde antes que cualquier conexión, que puede tardar o fallar. */
  readonly key = `tab-${++sequence}`;
  readonly profileId: string;
  /**
   * Contra qué base corre, y con qué nombre aparece en la barra. Son `$state` y no `readonly`
   * porque una pestaña de consulta puede cambiar de base sin cerrarse, y guardarla como archivo le
   * cambia el nombre. Las de datos y diagrama los fijan al abrirse y no los vuelven a tocar.
   */
  database = $state("");
  title = $state("");

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
  /**
   * La otra pestaña, cuando hay dos abiertas a la vez en el panel dividido. `null` = una sola
   * pestaña a la vista. No se persiste, igual que `active` y que `all`: la ventana ya abre siempre
   * sin pestañas, así que una pareja dividida tampoco tiene por qué sobrevivir a un reinicio.
   */
  split = $state<string | null>(null);

  get current(): Tab | null {
    return this.all.find((tab) => tab.key === this.active) ?? null;
  }

  /** Agrega la pestaña y la deja seleccionada. */
  add<T extends Tab>(tab: T): T {
    this.all.push(tab);
    this.activate(tab.key);
    return tab;
  }

  /**
   * Activa una pestaña (o el panel de Detalle, con `null`). Las dos mitades del panel dividido no
   * pueden mostrar la misma pestaña, así que activar la que está al lado la trae de vuelta a
   * pantalla completa en vez de dejar la otra mitad sin nada que mostrar.
   */
  activate(key: string | null) {
    this.active = key;
    if (key !== null && key === this.split) this.split = null;
  }

  /** Manda una pestaña al panel de al lado, o la saca si ya estaba ahí. */
  toggleSplit(key: string) {
    if (key === this.active) return;
    this.split = this.split === key ? null : key;
  }

  async close(key: string) {
    const tab = this.all.find((item) => item.key === key);
    if (!tab) return;

    this.all = this.all.filter((item) => item.key !== key);
    if (this.split === key) this.split = null;
    if (this.active === key) {
      this.active = this.all.at(-1)?.key ?? null;
    }
    // Cerraron las dos pestañas que había, o la única que quedaba pasa a ocupar las dos mitades.
    if (this.active !== null && this.active === this.split) this.split = null;

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
