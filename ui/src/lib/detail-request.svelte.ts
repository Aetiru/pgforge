/**
 * Un pedido del árbol al panel de detalle: «hacé esto sobre la fila que está elegida».
 *
 * Editar o borrar un rol ya tiene su camino completo en el detalle (carga del rol, formulario,
 * confirmación de producción, reasignación de lo que posee). Duplicarlo en el árbol significaba
 * dos lugares donde mantener las guardas; en cambio el menú contextual solo avisa qué acción es
 * y el detalle la ejecuta con las mismas reglas. Vive en un módulo y no en un prop porque el panel
 * puede estar oculto cuando se hace el pedido: se queda esperando hasta que se monte.
 */

import type { DetailActionKind } from "./detail-actions";

class DetailRequest {
  pending = $state<{ rowKey: string; kind: DetailActionKind } | null>(null);

  ask(rowKey: string, kind: DetailActionKind) {
    this.pending = { rowKey, kind };
  }

  clear() {
    this.pending = null;
  }
}

export const detailRequest = new DetailRequest();
