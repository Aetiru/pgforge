/**
 * Qué tabla tiene abierto el diálogo «Generar SQL». Lo piden dos puertas —el menú del árbol y el
 * panel de detalle— y lo dibuja `App.svelte`, así que el pedido pasa por acá y no por props.
 */
export interface GenerateTarget {
  profileId: string;
  database: string;
  oid: number;
  /** `esquema.tabla`, para el título. */
  title: string;
}

class GenerateSqlState {
  target = $state.raw<GenerateTarget | null>(null);

  open(target: GenerateTarget) {
    this.target = target;
  }

  close() {
    this.target = null;
  }
}

export const generateSqlDialog = new GenerateSqlState();
