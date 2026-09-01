/**
 * Qué muestra el panel lateral izquierdo.
 *
 * Antes esto era «qué vista principal se está mirando», y las cuatro vistas se tapaban entre sí:
 * ir a mirar el monitoreo se llevaba puestos el árbol y las pestañas, y volver a lo que uno estaba
 * haciendo costaba dos clics y acordarse de dónde estaba. Monitoreo y configuración pasaron a ser
 * pestañas —tienen un servidor adentro, igual que una consulta—, así que lo único excluyente que
 * queda es qué ocupa el panel lateral, que es un panel y no la pantalla.
 *
 * Sigue viviendo afuera de `App.svelte` por el mismo motivo de siempre: otras partes **llevan** a
 * un panel (un aviso de que terminó un proceso abre el de procesos) y pasarlo como `prop` por tres
 * niveles no lo hace más claro.
 */

import { dock } from "./dock.svelte";

export type SidePane = "explorer" | "library" | "processes";

class View {
  pane = $state<SidePane>("explorer");

  /** Lleva a un panel. Si estaba plegado se abre: nadie pide un panel para no verlo. */
  show(pane: SidePane) {
    this.pane = pane;
    dock.setSidebar(true);
  }

  /** Lo que hace el botón del riel: el panel que ya se está mirando se pliega. */
  toggle(pane: SidePane) {
    if (this.pane === pane && dock.sidebarOpen) {
      dock.setSidebar(false);
      return;
    }
    this.show(pane);
  }
}

export const view = new View();
