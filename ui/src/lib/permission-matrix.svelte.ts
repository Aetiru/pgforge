/**
 * La pestaña de la matriz de permisos: roles×objetos de un esquema elegido.
 *
 * Como la del diagrama, no toma nada del lado de Rust —trae el resultado de una lectura y lo
 * dibuja—, así que no hay sesión que soltar y `dispose()` queda como está en `Tab`. `schema` y
 * `objectKind` viven acá y no en el panel para sobrevivir a un cambio de pestaña: cambiar cualquiera
 * de los dos vuelve a leer, así que perderlos al mirar otra cosa obligaría a elegir todo de nuevo.
 */

import { Tab, tabs } from "./tabs.svelte";
import {
  describeError,
  roleNames,
  schemaFunctionPrivileges,
  schemaNames,
  schemaSequencePrivileges,
  schemaTablePrivileges,
  type EffectivePrivilege,
} from "./ipc";

export type MatrixObjectKind = "table" | "sequence" | "function";

const READERS: Record<
  MatrixObjectKind,
  (id: string, schema: string, roles: string[], database?: string) => Promise<EffectivePrivilege[]>
> = {
  table: schemaTablePrivileges,
  sequence: schemaSequencePrivileges,
  function: schemaFunctionPrivileges,
};

export class PermissionMatrixTab extends Tab {
  readonly kind = "permissionMatrix" as const;

  schema = $state<string>("");
  objectKind = $state<MatrixObjectKind>("table");

  /** Esquemas y roles disponibles, leídos una sola vez al abrir la pestaña. */
  schemas = $state<string[]>([]);
  availableRoles = $state<string[]>([]);
  /** Subconjunto de `availableRoles` que se muestra. Arranca en todos. */
  selectedRoles = $state<string[]>([]);

  effective = $state.raw<EffectivePrivilege[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  constructor(profileId: string, database: string, schema: string) {
    super(profileId, database, `Permisos · ${schema}`);
    this.schema = schema;
  }

  /** Esquemas y roles del servidor, para los selectores. Se pide una sola vez al abrir. */
  async loadOptions() {
    try {
      const [schemas, roles] = await Promise.all([
        schemaNames(this.profileId, this.database),
        roleNames(this.profileId, this.database),
      ]);
      this.schemas = schemas;
      this.availableRoles = roles;
      if (this.selectedRoles.length === 0) this.selectedRoles = roles;
    } catch (error) {
      this.error = describeError(error);
    }
  }

  toggleRole(role: string) {
    this.selectedRoles = this.selectedRoles.includes(role)
      ? this.selectedRoles.filter((r) => r !== role)
      : [...this.selectedRoles, role];
  }

  async load() {
    if (this.selectedRoles.length === 0) {
      this.effective = [];
      return;
    }
    this.loading = true;
    this.error = null;
    try {
      this.effective = await READERS[this.objectKind](
        this.profileId,
        this.schema,
        this.selectedRoles,
        this.database,
      );
    } catch (error) {
      this.error = describeError(error);
      this.effective = [];
    } finally {
      this.loading = false;
    }
  }
}

/** Abre la matriz de permisos de un esquema y trae la primera lectura (tablas). */
export async function openPermissionMatrix(
  profileId: string,
  database: string,
  schema: string,
): Promise<PermissionMatrixTab> {
  const tab = tabs.add(new PermissionMatrixTab(profileId, database, schema));
  await tab.loadOptions();
  await tab.load();
  return tab;
}
