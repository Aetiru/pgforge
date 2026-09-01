/**
 * La pestaña de configuración de un servidor (`pg_settings`).
 *
 * Misma mudanza que el dashboard: era una vista excluyente con su propio `<select>` de servidor
 * arriba, y ahora el servidor viaja adentro de la pestaña. A diferencia del monitoreo, acá no hay
 * ninguna instancia única debajo —`ServerConfig` lee las opciones y no sondea nada—, así que se
 * pueden tener dos abiertas contra dos servidores y compararlas lado a lado en el panel dividido.
 */

import { Tab, tabs } from "./tabs.svelte";

export class ConfigTab extends Tab {
  readonly kind = "config" as const;

  constructor(profileId: string) {
    super(profileId, "", "Configuración");
  }
}

/** Abre la configuración de un servidor, o vuelve a la que ya estaba abierta para ese servidor. */
export function openConfig(profileId: string): ConfigTab {
  const open = tabs.all.find(
    (tab): tab is ConfigTab => tab instanceof ConfigTab && tab.profileId === profileId,
  );
  if (open) {
    tabs.activate(open.key);
    return open;
  }
  return tabs.add(new ConfigTab(profileId));
}
