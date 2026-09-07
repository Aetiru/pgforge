/**
 * La pestaña "qué puede hacer este rol": mismo primitivo que la matriz de permisos
 * (`permission-matrix.svelte.ts`), pedido para un solo rol y las tres familias de objetos juntas en
 * vez de una a la vez — acá no hay selector de tipo de objeto porque el punto es ver todo lo que el
 * rol toca en el esquema elegido, no comparar un tipo entre roles.
 */

import { Tab, tabs } from "./tabs.svelte";
import {
  describeError,
  schemaFunctionPrivileges,
  schemaNames,
  schemaSequencePrivileges,
  schemaTablePrivileges,
  type EffectivePrivilege,
} from "./ipc";

export class RoleCapabilitiesTab extends Tab {
  readonly kind = "roleCapabilities" as const;

  readonly role: string;
  schema = $state<string>("");
  schemas = $state<string[]>([]);

  tables = $state.raw<EffectivePrivilege[]>([]);
  sequences = $state.raw<EffectivePrivilege[]>([]);
  functions = $state.raw<EffectivePrivilege[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  constructor(profileId: string, database: string, role: string, schema: string) {
    super(profileId, database, `Rol · ${role}`);
    this.role = role;
    this.schema = schema;
  }

  async loadOptions() {
    try {
      this.schemas = await schemaNames(this.profileId, this.database);
    } catch (error) {
      this.error = describeError(error);
    }
  }

  async load() {
    this.loading = true;
    this.error = null;
    try {
      const roles = [this.role];
      const [tables, sequences, functions] = await Promise.all([
        schemaTablePrivileges(this.profileId, this.schema, roles, this.database),
        schemaSequencePrivileges(this.profileId, this.schema, roles, this.database),
        schemaFunctionPrivileges(this.profileId, this.schema, roles, this.database),
      ]);
      this.tables = tables;
      this.sequences = sequences;
      this.functions = functions;
    } catch (error) {
      this.error = describeError(error);
      this.tables = [];
      this.sequences = [];
      this.functions = [];
    } finally {
      this.loading = false;
    }
  }
}

/** Abre "qué puede hacer" un rol contra el esquema indicado (`public` por omisión). */
export async function openRoleCapabilities(
  profileId: string,
  database: string,
  role: string,
  schema = "public",
): Promise<RoleCapabilitiesTab> {
  const tab = tabs.add(new RoleCapabilitiesTab(profileId, database, role, schema));
  await tab.loadOptions();
  await tab.load();
  return tab;
}
