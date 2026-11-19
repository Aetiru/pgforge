/** Qué esquema tiene abierto el diálogo «Documentar el esquema». Lo dibuja `App.svelte`. */
export interface SchemaDocTarget {
  profileId: string;
  database: string;
  schema: string;
}

class SchemaDocState {
  target = $state.raw<SchemaDocTarget | null>(null);

  open(target: SchemaDocTarget) {
    this.target = target;
  }

  close() {
    this.target = null;
  }
}

export const schemaDocDialog = new SchemaDocState();
