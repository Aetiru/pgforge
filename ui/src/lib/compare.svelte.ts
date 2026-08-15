/**
 * La pestaña de comparación de esquemas.
 *
 * Como la del diagrama, no toma nada del lado de Rust: trae el resultado de una lectura y lo dibuja,
 * así que no hay sesión que soltar y `dispose()` queda como está en `Tab`. Lo que sí guarda es
 * contra qué se comparó, para poder volver a comparar sin volver a preguntarlo: después de correr
 * el script uno quiere ver que ya no queda nada, y ese es el botón que más se usa.
 */

import { Tab, tabs } from "./tabs.svelte";
import { describeError, schemaCompare, type CompareSide, type Comparison } from "./ipc";
import { DEFAULT_RISKS, type RiskFilter } from "./compare-script";

export class CompareTab extends Tab {
  readonly kind = "compare" as const;

  readonly source: CompareSide;
  readonly target: CompareSide;

  result = $state.raw<Comparison | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  /** Qué entra en el script. Vive en la pestaña y no en el panel para sobrevivir a un refresco. */
  risks = $state<RiskFilter>({ ...DEFAULT_RISKS });
  /** Nombre del objeto cuyo detalle está abierto en el informe, o `null`. */
  opened = $state<string | null>(null);

  constructor(source: CompareSide, target: CompareSide) {
    // La pestaña cuelga del origen: es el servidor cuyo estado se quiere copiar, y es el que decide
    // qué se cierra si esa conexión se cae.
    super(source.id, source.database, `Comparar · ${source.schema} → ${target.schema}`);
    this.source = source;
    this.target = target;
  }

  async load() {
    this.loading = true;
    this.error = null;
    try {
      this.result = await schemaCompare(this.source, this.target);
    } catch (error) {
      this.error = describeError(error);
      this.result = null;
    } finally {
      this.loading = false;
    }
  }
}

/** Abre la comparación de dos esquemas y la corre. */
export async function openCompare(
  source: CompareSide,
  target: CompareSide,
): Promise<CompareTab> {
  const tab = tabs.add(new CompareTab(source, target));
  await tab.load();
  return tab;
}
