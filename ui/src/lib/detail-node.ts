/**
 * Lo que se sabe del nodo elegido sin preguntarle nada al servidor.
 *
 * `DetailPanel` tenía todo esto adentro: treinta banderas derivadas, el vocabulario de cada objeto y
 * los agrupadores de privilegios. Acá son funciones puras y se prueban con Vitest, que es lo que no
 * se puede hacer con un `$derived` metido en un componente de tres mil líneas.
 */

import { folderOf, formatVersion, type TreeNode } from "./ipc";
import type {
  ColumnGrant,
  CommentTarget,
  ConnectionProfile,
  PrivilegeGrant,
  ServerCaps,
  TriggerInfo,
} from "./ipc";
import { envLook } from "./badges";
import type { Subject as PrivilegeSubject } from "./PrivilegeDialog.svelte";

/** Qué es el nodo elegido. Una bandera por pregunta que el panel se hace más de una vez. */
export interface NodeFlags {
  isTable: boolean;
  isView: boolean;
  isMaterializedView: boolean;
  isFunction: boolean;
  isProcedure: boolean;
  isRoutine: boolean;
  isRole: boolean;
  isExtension: boolean;
  isFdw: boolean;
  isForeignServer: boolean;
  isSchema: boolean;
  isSequence: boolean;
  isDatabase: boolean;
  /** Los dominios también son `pg_type`, así que el árbol los trae con esta misma clase. */
  isType: boolean;
  isPartitionedTable: boolean;
  isTablesFolder: boolean;
  isViewsFolder: boolean;
  isMatViewsFolder: boolean;
  isFunctionsFolder: boolean;
  isProceduresFolder: boolean;
  /** La única carpeta que no cuelga de una base: es hermana de todas ellas en la raíz. */
  isRolesFolder: boolean;
  isExtensionsFolder: boolean;
  isFdwsFolder: boolean;
  isForeignServersFolder: boolean;
  isSchemasFolder: boolean;
  isSequencesFolder: boolean;
  isTypesFolder: boolean;
  isFolder: boolean;
  /**
   * Los tipos de objeto que tienen privilegios propios. Los índices y las restricciones no están:
   * no tienen ACL, heredan el de la tabla.
   */
  hasPrivileges: boolean;
  /** Ni las carpetas, ni las bases, ni la fila del servidor tienen un DDL propio. */
  hasDdl: boolean;
}

const NONE: NodeFlags = {
  isTable: false,
  isView: false,
  isMaterializedView: false,
  isFunction: false,
  isProcedure: false,
  isRoutine: false,
  isRole: false,
  isExtension: false,
  isFdw: false,
  isForeignServer: false,
  isSchema: false,
  isSequence: false,
  isDatabase: false,
  isType: false,
  isPartitionedTable: false,
  isTablesFolder: false,
  isViewsFolder: false,
  isMatViewsFolder: false,
  isFunctionsFolder: false,
  isProceduresFolder: false,
  isRolesFolder: false,
  isExtensionsFolder: false,
  isFdwsFolder: false,
  isForeignServersFolder: false,
  isSchemasFolder: false,
  isSequencesFolder: false,
  isTypesFolder: false,
  isFolder: false,
  hasPrivileges: false,
  hasDdl: false,
};

export function flagsOf(node: TreeNode | null): NodeFlags {
  if (!node) return NONE;

  const folder = folderOf(node.kind);
  const isTable = node.kind === "table" || node.kind === "partitionedTable";
  const isView = node.kind === "view";
  const isMaterializedView = node.kind === "materializedView";
  const isFunction = node.kind === "function";
  const isProcedure = node.kind === "procedure";
  const isRoutine = isFunction || isProcedure;
  const isSchema = node.kind === "schema";
  const isSequence = node.kind === "sequence";
  const isDatabase = node.kind === "database";

  return {
    isTable,
    isView,
    isMaterializedView,
    isFunction,
    isProcedure,
    isRoutine,
    isRole: node.kind === "role",
    isExtension: node.kind === "extension",
    isFdw: node.kind === "foreignDataWrapper",
    isForeignServer: node.kind === "foreignServer",
    isSchema,
    isSequence,
    isDatabase,
    isType: node.kind === "type",
    isPartitionedTable: node.kind === "partitionedTable",
    isTablesFolder: folder === "tables",
    isViewsFolder: folder === "views",
    isMatViewsFolder: folder === "materializedViews",
    isFunctionsFolder: folder === "functions",
    isProceduresFolder: folder === "procedures",
    isRolesFolder: folder === "roles",
    isExtensionsFolder: folder === "extensions",
    isFdwsFolder: folder === "fdws",
    isForeignServersFolder: folder === "fservers",
    isSchemasFolder: folder === "schemas",
    isSequencesFolder: folder === "sequences",
    isTypesFolder: folder === "types",
    isFolder: folder !== null,
    hasPrivileges:
      isTable || isSchema || isSequence || isDatabase || isView || isMaterializedView || isRoutine,
    hasDdl: folder === null && !isDatabase,
  };
}

/** Sobre qué objeto se comenta, a partir del nodo seleccionado. */
export function commentTargetOf(node: TreeNode | null): CommentTarget | null {
  if (!node) return null;
  const schema = node.schema ?? "public";

  switch (node.kind) {
    case "table":
    case "partitionedTable":
      return { kind: "table", schema, name: node.label };
    case "foreignTable":
      return { kind: "foreignTable", schema, name: node.label };
    case "view":
      return { kind: "view", schema, name: node.label };
    case "materializedView":
      return { kind: "materializedView", schema, name: node.label };
    case "sequence":
      return { kind: "sequence", schema, name: node.label };
    case "index":
      return { kind: "index", schema, name: node.label };
    case "type":
      return { kind: "type", schema, name: node.label };
    case "schema":
      return { kind: "schema", name: node.label };
    case "database":
      return { kind: "database", name: node.label };
    case "role":
      return { kind: "role", name: node.label };
    case "extension":
      return { kind: "extension", name: node.label };
    default:
      return null;
  }
}

/** El objeto del que habla el diálogo de privilegios, con el vocabulario que le corresponde. */
export function privilegeSubjectOf(
  node: TreeNode | null,
  flags: NodeFlags,
  routineArgs: string,
): PrivilegeSubject | null {
  if (!node) return null;
  if (flags.isDatabase) return { on: "database", database: node.label };
  if (flags.isSchema) return { on: "schema", schema: node.label };
  if (!node.schema) return null;
  if (flags.isSequence) return { on: "sequence", schema: node.schema, sequence: node.label };
  if (flags.isRoutine) {
    return {
      on: "function",
      schema: node.schema,
      name: node.label,
      args: routineArgs,
      procedure: flags.isProcedure,
    };
  }
  // Las vistas y las materializadas comparten el vocabulario de una tabla.
  if (flags.isTable || flags.isView || flags.isMaterializedView) {
    return { on: "table", schema: node.schema, table: node.label };
  }
  return null;
}

export interface PrivilegeGroup {
  grantee: string;
  privileges: string[];
  grantable: boolean;
}

/** Una fila de `aclexplode` por privilegio: se agrupan por `grantee` para mostrar una sola línea. */
export function privilegeGroupsOf(privileges: PrivilegeGrant[] | null): PrivilegeGroup[] {
  if (!privileges) return [];
  const byGrantee = new Map<string, PrivilegeGroup>();
  for (const grant of privileges) {
    const group = byGrantee.get(grant.grantee);
    const privilege = grant.privilege.toLowerCase();
    if (group) {
      group.privileges.push(privilege);
      if (grant.grantable) group.grantable = true;
    } else {
      byGrantee.set(grant.grantee, {
        grantee: grant.grantee,
        privileges: [privilege],
        grantable: grant.grantable,
      });
    }
  }
  return [...byGrantee.values()];
}

/**
 * La clave de una fila de privilegios por columna. Va por JSON y no concatenando con un separador:
 * un nombre de columna puede tener cualquier cosa adentro, incluido el separador.
 */
export function pairKey(column: string, grantee: string): string {
  return JSON.stringify([column, grantee]);
}

export interface ColumnGroup {
  column: string;
  grantee: string;
  privileges: string[];
}

/** Lo mismo que `privilegeGroupsOf`, pero la fila es la combinación de columna y destinatario. */
export function columnGroupsOf(columnGrants: ColumnGrant[]): ColumnGroup[] {
  const byPair = new Map<string, ColumnGroup>();
  for (const grant of columnGrants) {
    const key = pairKey(grant.column, grant.grantee);
    const group = byPair.get(key);
    if (group) {
      group.privileges.push(grant.privilege);
    } else {
      byPair.set(key, {
        column: grant.column,
        grantee: grant.grantee,
        privileges: [grant.privilege],
      });
    }
  }
  return [...byPair.values()];
}

const TIMING_LABEL: Record<TriggerInfo["timing"], string> = {
  before: "BEFORE",
  after: "AFTER",
  insteadOf: "INSTEAD OF",
};

const EVENT_LABEL: Record<TriggerInfo["events"][number], string> = {
  insert: "INSERT",
  update: "UPDATE",
  delete: "DELETE",
  truncate: "TRUNCATE",
};

export function triggerSummary(trigger: TriggerInfo): string {
  const events = trigger.events.map((event) => EVENT_LABEL[event]).join(" OR ");
  return `${TIMING_LABEL[trigger.timing]} ${events} · ${trigger.level === "row" ? "ROW" : "STATEMENT"}`;
}

export interface Property {
  label: string;
  value: string;
  /** Se pinta en ámbar: es un permiso que falta, o un servidor marcado como producción. */
  bad?: boolean;
}

/** Lo que no cabe en el encabezado: los datos de la conexión, o los del objeto. */
export function propertiesOf(
  isServer: boolean,
  node: TreeNode | null,
  profile: ConnectionProfile | null,
  caps: ServerCaps | null,
): Property[] {
  if (!isServer) {
    if (!node) return [];
    const rows: Property[] = [{ label: "Base de datos", value: node.database }];
    if (node.schema) rows.push({ label: "Esquema", value: node.schema });
    if (node.oid) rows.push({ label: "OID", value: String(node.oid) });
    return rows;
  }
  if (!profile) return [];

  const rows: Property[] = [
    { label: "Servidor", value: `${profile.host}:${profile.port}` },
    { label: "Base inicial", value: profile.database },
    { label: "Usuario", value: profile.user },
    { label: "Cifrado", value: profile.sslMode },
    {
      label: "Entorno",
      value: profile.environment ? envLook(profile.environment).title : "sin marcar",
      bad: profile.environment === "prod",
    },
    { label: "Solo lectura", value: profile.readOnly ? "sí" : "no" },
    { label: "Autocommit", value: profile.autocommit ? "sí" : "no" },
  ];

  if (caps) {
    rows.push(
      { label: "Versión", value: `PostgreSQL ${formatVersion(caps.version)}` },
      { label: "Superusuario", value: caps.isSuperuser ? "sí" : "no" },
      {
        label: "Puede cancelar sesiones",
        value: caps.canSignalBackends ? "sí" : "no (falta pg_signal_backend)",
        bad: !caps.canSignalBackends,
      },
      {
        label: "Ve todas las estadísticas",
        value: caps.canReadAllStats ? "sí" : "no (falta pg_read_all_stats)",
        bad: !caps.canReadAllStats,
      },
    );
  }
  return rows;
}

/** La ruta del objeto: base / esquema. Contesta «¿de dónde salió esto?» sin volver al árbol. */
export function pathOf(
  isServer: boolean,
  node: TreeNode | null,
  profile: ConnectionProfile | null,
): string {
  if (isServer) return profile ? `${profile.host}:${profile.port}` : "";
  if (!node) return "";
  return [node.database, node.schema].filter(Boolean).join(" / ");
}

/** Un punto de partida mínimo: mejor que una pantalla en blanco, sin fingir saber qué necesita. */
export function functionSkeleton(schema: string, procedure: boolean): string {
  return procedure
    ? `CREATE PROCEDURE ${schema}.nombre()\nLANGUAGE plpgsql\nAS $$\nBEGIN\nEND;\n$$;`
    : `CREATE FUNCTION ${schema}.nombre()\nRETURNS void\nLANGUAGE plpgsql\nAS $$\nBEGIN\nEND;\n$$;`;
}
