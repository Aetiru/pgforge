/**
 * Qué vista principal se está mirando.
 *
 * Estaba adentro de `App.svelte` mientras fue cosa suya: son cuatro botones de una barra. Dejó de
 * serlo cuando otras partes empezaron a **llevar** a una vista —el dashboard abre una consulta y hay
 * que ir a verla, la vista de procesos se muestra sola cuando algo termina—, y pasarlo como
 * `prop` por tres niveles para eso no lo hace más claro.
 */

export type MainView = "explorer" | "monitor" | "config" | "processes";

class View {
  current = $state<MainView>("explorer");

  show(view: MainView) {
    this.current = view;
  }
}

export const view = new View();
