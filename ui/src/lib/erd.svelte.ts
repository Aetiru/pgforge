/**
 * La pestaña del diagrama ERD.
 *
 * A diferencia de las otras dos, no toma nada del lado de Rust: el grafo se trae una vez y se
 * dibuja en el cliente, así que no hay sesión que soltar y `dispose()` queda como está en `Tab`.
 * Lo que sí vive acá es lo que el usuario movió: las posiciones arrastradas sobreviven a un
 * refresco del grafo, porque perderlas después de acomodar el diagrama a mano sería el peor
 * momento para reordenarlo.
 */

import { Tab, tabs } from "./tabs.svelte";
import { describeError, schemaGraph, type SchemaGraph } from "./ipc";
import type { Positions } from "./erd";

export class ErdTab extends Tab {
  readonly kind = "erd" as const;

  readonly schema: string;

  graph = $state.raw<SchemaGraph | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);

  /** Posiciones arrastradas por OID. Vacío significa que manda el layout calculado. */
  moved = $state.raw<Positions>({});
  /** Tabla resaltada, o `null`. */
  selected = $state<number | null>(null);

  constructor(profileId: string, database: string, schema: string) {
    super(profileId, database, `ERD · ${schema}`);
    this.schema = schema;
  }

  async load() {
    this.loading = true;
    this.error = null;
    try {
      this.graph = await schemaGraph(this.profileId, this.database, this.schema);
    } catch (error) {
      this.error = describeError(error);
    } finally {
      this.loading = false;
    }
  }

  move(oid: number, x: number, y: number) {
    this.moved = { ...this.moved, [oid]: { x, y } };
  }

  /** Vuelve al layout calculado, descartando lo arrastrado. */
  reset() {
    this.moved = {};
  }
}

/** Abre el diagrama de un esquema y trae su grafo. */
export async function openErd(
  profileId: string,
  database: string,
  schema: string,
): Promise<ErdTab> {
  const tab = tabs.add(new ErdTab(profileId, database, schema));
  await tab.load();
  return tab;
}
