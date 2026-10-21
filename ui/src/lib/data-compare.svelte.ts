/**
 * La pestaña de comparación de datos entre dos tablas.
 *
 * A diferencia de la de esquemas, la lectura puede tardar (recorre las dos tablas enteras), así que
 * lleva avance y se puede cancelar: `requestId` registra del lado de Rust las dos lecturas y
 * `dataCompareCancel` las aborta en los dos servidores. Sigue sin ejecutar nada: el script se copia
 * o se abre en una consulta contra el destino.
 */

import { Tab, tabs } from "./tabs.svelte";
import {
  Channel,
  dataCompare,
  dataCompareCancel,
  describeError,
  isCanceled,
  type DataComparison,
  type DataCompareEvent,
  type DataSide,
} from "./ipc";
import { DEFAULT_RISKS, type RiskFilter } from "./compare-script";

export class DataCompareTab extends Tab {
  readonly kind = "dataCompare" as const;

  readonly source: DataSide;
  readonly target: DataSide;

  result = $state.raw<DataComparison | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  /** Filas leídas hasta ahora de cada lado, mientras corre. */
  progress = $state({ sourceRows: 0, targetRows: 0 });

  /** Qué entra en el script. Lo destructivo (los DELETE) arranca apagado. */
  risks = $state<RiskFilter>({ ...DEFAULT_RISKS });

  private requestId: string | null = null;

  constructor(source: DataSide, target: DataSide) {
    // Cuelga del origen, igual que la comparación de esquemas: es el servidor cuya caída decide
    // qué se cierra.
    super(source.id, source.database, `Datos · ${source.table} → ${target.table}`);
    this.source = source;
    this.target = target;
  }

  async load() {
    if (this.loading) return;
    const requestId = crypto.randomUUID();
    this.requestId = requestId;
    this.loading = true;
    this.error = null;
    this.progress = { sourceRows: 0, targetRows: 0 };

    const channel = new Channel<DataCompareEvent>();
    channel.onmessage = (event) => {
      if (event.type === "progress") {
        this.progress = { sourceRows: event.sourceRows, targetRows: event.targetRows };
      }
    };

    try {
      this.result = await dataCompare(this.source, this.target, requestId, channel);
    } catch (error) {
      // Cancelar no es una falla: se deja lo que hubiera, sin cartel rojo.
      if (!isCanceled(error)) {
        this.error = describeError(error);
        this.result = null;
      }
    } finally {
      this.loading = false;
      this.requestId = null;
    }
  }

  /** Aborta la lectura en curso, del lado del servidor. */
  async cancel() {
    if (this.requestId) await dataCompareCancel(this.requestId).catch(() => {});
  }

  async dispose() {
    await this.cancel();
  }
}

/** Abre la comparación de datos de dos tablas y la corre. */
export async function openDataCompare(
  source: DataSide,
  target: DataSide,
): Promise<DataCompareTab> {
  const tab = tabs.add(new DataCompareTab(source, target));
  await tab.load();
  return tab;
}
