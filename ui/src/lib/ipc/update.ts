/**
 * Aviso de versión nueva. Espejo de `pgforge_core::update`.
 *
 * No hay descarga ni instalación: el núcleo pregunta a la API de releases de GitHub y, si hay algo
 * más nuevo, `updateOpen` abre esa página en el navegador del sistema.
 */

import { invoke } from "./core";

export interface Release {
  /** Sin la `v` del tag: `0.1.5`. */
  version: string;
  name: string;
  /** Markdown tal como lo escribió quien publicó; se muestra como texto. */
  notes: string;
  url: string;
  publishedAt?: string | null;
}

export interface UpdateCheck {
  current: string;
  /** `null` cuando lo que corre ya es lo último, que es el caso normal. */
  newer?: Release | null;
}

export const updateCheck = () => invoke<UpdateCheck>("update_check");

/** Abre la página de la release. El núcleo rechaza cualquier dirección ajena al repositorio. */
export const updateOpen = (url: string) => invoke<void>("update_open", { url });
