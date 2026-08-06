<script lang="ts">
  import { untrack } from "svelte";
  import Alert from "./Alert.svelte";
  import BackupDialog from "./BackupDialog.svelte";
  import RestoreDialog from "./RestoreDialog.svelte";
  import ExportDialog from "./ExportDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import ColumnDialog from "./ColumnDialog.svelte";
  import Confirm from "./Confirm.svelte";
  import ConstraintDialog from "./ConstraintDialog.svelte";
  import Empty from "./Empty.svelte";
  import ExtensionDialog from "./ExtensionDialog.svelte";
  import FdwDialog from "./FdwDialog.svelte";
  import ForeignServerDialog from "./ForeignServerDialog.svelte";
  import FunctionDialog from "./FunctionDialog.svelte";
  import UserMappingDialog from "./UserMappingDialog.svelte";
  import Icon from "./Icon.svelte";
  import IndexDialog from "./IndexDialog.svelte";
  import PolicyDialog from "./PolicyDialog.svelte";
  import PrivilegeDialog, {
    type Existing as PrivilegeExisting,
    type Subject as PrivilegeSubject,
  } from "./PrivilegeDialog.svelte";
  import RoleDialog from "./RoleDialog.svelte";
  import FontSize from "./FontSize.svelte";
  import Sql from "./Sql.svelte";
  import TableDialog from "./TableDialog.svelte";
  import TriggerDialog from "./TriggerDialog.svelte";
  import CommentDialog from "./CommentDialog.svelte";
  import DatabaseDialog from "./DatabaseDialog.svelte";
  import DomainDialog from "./DomainDialog.svelte";
  import PartitionDialog from "./PartitionDialog.svelte";
  import SchemaDialog from "./SchemaDialog.svelte";
  import SequenceDialog from "./SequenceDialog.svelte";
  import TypeDialog from "./TypeDialog.svelte";
  import ViewDialog from "./ViewDialog.svelte";
  import { confirmMutation, isReadOnly, readOnlyReason } from "./access.svelte";
  import { envLook, GROUP_LOOK, kindLabel, lookOf } from "./badges";
  import { explorer, type Row } from "./explorer.svelte";
  import {
    dataOpen,
    databaseApply,
    ddlApply,
    describeError,
    domainApply,
    extensionApply,
    extensionInfo,
    fdwApply,
    fdwInfo,
    foreignServerApply,
    foreignServerInfo,
    userMappingApply,
    userMappings,
    folderOf,
    formatVersion,
    functionArgs,
    functionDrop,
    indexDrop,
    columnPrivileges,
    databasePrivileges,
    defaultPrivileges,
    functionPrivileges,
    objectDdl,
    policyApply,
    privilegeApply,
    relationPrivileges,
    roleApply,
    roleInfo,
    schemaApply,
    schemaPrivileges,
    sequenceApply,
    tableConstraints,
    tableIndexes,
    tablePartitions,
    tableSecurity,
    tableTriggers,
    triggerApply,
    typeApply,
    typeInfo,
    viewApply,
    type ColumnGrant,
    type CommentTarget,
    type ConstraintInfo,
    type Ddl,
    type DefaultGrant,
    type Grantable,
    type IndexInfo,
    type PolicyChange,
    type PolicyInfo,
    type ExtensionInfo,
    type FdwInfo,
    type ServerInfo,
    type UserMapping,
    type PrivilegeGrant,
    type RoleChange,
    type RoleInfo,
    type TableColumn,
    type TableSecurity,
    type TableShape,
    type TriggerInfo,
  } from "./ipc";

  let {
    onedit,
    ondelete,
    onconnect,
    ongroup,
    onquery,
    ondata,
    onerd,
  }: {
    onedit: (profileId: string) => void;
    ondelete: (profileId: string) => void;
    onconnect: (profileId: string) => void;
    /** Abre el diálogo de la carpeta de conexiones seleccionada. */
    ongroup: (name: string) => void;
    onquery: (profileId: string, database: string, title: string) => void;
    ondata: (profileId: string, database: string, title: string, oid: number) => void;
    /** Abre el diagrama ERD de un esquema. */
    onerd: (profileId: string, database: string, schema: string) => void;
  } = $props();

  let ddl = $state<Ddl | null>(null);
  let ddlError = $state<string | null>(null);
  let loading = $state(false);
  let copied = $state(false);

  const selected = $derived(explorer.selected);
  const node = $derived(selected?.node ?? null);
  const isServer = $derived(selected?.kind === "server");
  /** Una carpeta de conexiones: agrupa servidores guardados y no existe en ninguna base. */
  const isGroup = $derived(selected?.kind === "group");
  const groupServers = $derived(
    isGroup ? explorer.servers.filter((row) => row.group === selected!.group) : [],
  );
  const profile = $derived(
    selected ? (explorer.profiles.find((item) => item.id === selected.profileId) ?? null) : null,
  );
  const caps = $derived(selected ? (explorer.caps[selected.profileId] ?? null) : null);
  const look = $derived(isGroup ? GROUP_LOOK : lookOf(node?.kind ?? null));

  /**
   * Lo que se le agrega a un botón que modifica algo cuando el servidor es de solo lectura.
   *
   * Se esparce con `{...blocked}` después del `title` para que gane el motivo: un botón apagado sin
   * explicación se lee como una falla de la aplicación, no como una decisión del perfil.
   */
  const blocked = $derived(
    selected && isReadOnly(selected.profileId)
      ? { disabled: true, title: readOnlyReason(selected.profileId) ?? "" }
      : {},
  );

  /** Ni las carpetas, ni las bases, ni la fila del servidor tienen un DDL propio. */
  const hasDdl = $derived(node !== null && folderOf(node.kind) === null && node.kind !== "database");

  $effect(() => {
    const current = node;
    ddl = null;
    ddlError = null;
    copied = false;

    if (!current || !hasDdl || !selected) return;

    const profileId = selected.profileId;
    let cancelled = false;
    loading = true;

    objectDdl(profileId, current)
      .then((result) => {
        if (!cancelled) ddl = result;
      })
      .catch((error) => {
        if (!cancelled) ddlError = describeError(error);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });

    // Cambiar de nodo rápido no debe dejar que una respuesta vieja pise a la nueva.
    return () => {
      cancelled = true;
    };
  });

  /**
   * Re-lee el DDL del nodo actual sin esperar a que cambie de nodo. Hace falta después de editar
   * una vista: el nodo sigue siendo el mismo, así que el efecto de arriba no se vuelve a disparar
   * solo.
   */
  async function refreshDdl() {
    if (!node || !hasDdl || !selected) return;
    loading = true;
    ddlError = null;
    try {
      ddl = await objectDdl(selected.profileId, node);
    } catch (error) {
      ddlError = describeError(error);
    } finally {
      loading = false;
    }
  }

  async function copy() {
    if (!ddl) return;
    await navigator.clipboard.writeText(ddl.sql);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  /**
   * Contra qué base abriría una consulta lo que está seleccionado. Los objetos la llevan encima;
   * la fila del servidor recién conectado usa la del perfil.
   */
  const queryTarget = $derived.by<{ database: string; title: string } | null>(() => {
    if (!selected) return null;
    if (node) return { database: node.database, title: node.label };
    if (selected.connected && profile) {
      return { database: profile.database, title: profile.name };
    }
    return null;
  });

  /**
   * Las relaciones que tienen filas para mostrar. Las vistas y las materializadas entran: se abren
   * en solo lectura, y el propio panel explica por qué.
   */
  const dataTarget = $derived.by<number | null>(() => {
    const kinds = ["table", "partitionedTable", "view", "materializedView", "foreignTable"];
    if (!node || typeof node.kind !== "string" || !kinds.includes(node.kind)) return null;
    return node.oid ?? null;
  });

  /** Solo las tablas (particionadas o no) tienen columnas que se puedan crear, cambiar o borrar. */
  const isTable = $derived(node?.kind === "table" || node?.kind === "partitionedTable");
  /** El nodo carpeta "Tablas" de un esquema, donde vive el botón para crear una tabla nueva. */
  const isTablesFolder = $derived(node !== null && folderOf(node.kind) === "tables");

  const isView = $derived(node?.kind === "view");
  const isMaterializedView = $derived(node?.kind === "materializedView");
  const isViewsFolder = $derived(node !== null && folderOf(node.kind) === "views");
  const isMatViewsFolder = $derived(node !== null && folderOf(node.kind) === "materializedViews");

  const isFunction = $derived(node?.kind === "function");
  const isProcedure = $derived(node?.kind === "procedure");
  const isRoutine = $derived(isFunction || isProcedure);
  const isFunctionsFolder = $derived(node !== null && folderOf(node.kind) === "functions");
  const isProceduresFolder = $derived(node !== null && folderOf(node.kind) === "procedures");

  const isRole = $derived(node?.kind === "role");
  /** La única carpeta que no cuelga de una base: es hermana de todas ellas en la raíz. */
  const isRolesFolder = $derived(node !== null && folderOf(node.kind) === "roles");

  const isExtension = $derived(node?.kind === "extension");
  const isExtensionsFolder = $derived(node !== null && folderOf(node.kind) === "extensions");

  const isFdw = $derived(node?.kind === "foreignDataWrapper");
  const isFdwsFolder = $derived(node !== null && folderOf(node.kind) === "fdws");
  const isForeignServer = $derived(node?.kind === "foreignServer");
  const isForeignServersFolder = $derived(node !== null && folderOf(node.kind) === "fservers");

  const isSchema = $derived(node?.kind === "schema");
  const isSequence = $derived(node?.kind === "sequence");
  const isDatabase = $derived(node?.kind === "database");
  /** Los dominios también son `pg_type`, así que el árbol los trae con esta misma clase. */
  const isType = $derived(node?.kind === "type");
  const isPartitionedTable = $derived(node?.kind === "partitionedTable");

  const isSchemasFolder = $derived(node !== null && folderOf(node.kind) === "schemas");
  const isSequencesFolder = $derived(node !== null && folderOf(node.kind) === "sequences");
  const isTypesFolder = $derived(node !== null && folderOf(node.kind) === "types");

  /**
   * Los tipos de objeto que tienen privilegios propios. Los índices y las restricciones no están:
   * no tienen ACL, heredan el de la tabla.
   */
  const hasPrivileges = $derived(
    isTable || isSchema || isSequence || isDatabase || isView || isMaterializedView || isRoutine,
  );

  const isFolder = $derived(node !== null && folderOf(node.kind) !== null);

  // -------------------------------------------------------------------------
  // Estructura: columnas de la tabla seleccionada
  // -------------------------------------------------------------------------

  let shape = $state<TableShape | null>(null);
  let shapeError = $state<string | null>(null);
  let shapeLoading = $state(false);

  async function loadShape() {
    if (!isTable || !node?.oid || !selected) {
      shape = null;
      return;
    }
    shapeLoading = true;
    shapeError = null;
    try {
      shape = await dataOpen(selected.profileId, node.oid, node.database);
    } catch (error) {
      shapeError = describeError(error);
    } finally {
      shapeLoading = false;
    }
  }

  let indexes = $state<IndexInfo[] | null>(null);
  let indexesError = $state<string | null>(null);
  let indexesLoading = $state(false);

  async function loadIndexes() {
    if (!isTable || !node?.oid || !selected) {
      indexes = null;
      return;
    }
    indexesLoading = true;
    indexesError = null;
    try {
      indexes = await tableIndexes(selected.profileId, node.oid, node.database);
    } catch (error) {
      indexesError = describeError(error);
    } finally {
      indexesLoading = false;
    }
  }

  let constraints = $state<ConstraintInfo[] | null>(null);
  let constraintsError = $state<string | null>(null);
  let constraintsLoading = $state(false);

  async function loadConstraints() {
    if (!isTable || !node?.oid || !selected) {
      constraints = null;
      return;
    }
    constraintsLoading = true;
    constraintsError = null;
    try {
      constraints = await tableConstraints(selected.profileId, node.oid, node.database);
    } catch (error) {
      constraintsError = describeError(error);
    } finally {
      constraintsLoading = false;
    }
  }

  let triggers = $state<TriggerInfo[] | null>(null);
  let triggersError = $state<string | null>(null);
  let triggersLoading = $state(false);

  async function loadTriggers() {
    if (!isTable || !node?.oid || !selected) {
      triggers = null;
      return;
    }
    triggersLoading = true;
    triggersError = null;
    try {
      triggers = await tableTriggers(selected.profileId, node.oid, node.database);
    } catch (error) {
      triggersError = describeError(error);
    } finally {
      triggersLoading = false;
    }
  }

  let security = $state<TableSecurity | null>(null);
  let securityError = $state<string | null>(null);
  let securityLoading = $state(false);

  async function loadSecurity() {
    if (!isTable || !node?.oid || !selected) {
      security = null;
      return;
    }
    securityLoading = true;
    securityError = null;
    try {
      security = await tableSecurity(selected.profileId, node.oid, node.database);
    } catch (error) {
      securityError = describeError(error);
    } finally {
      securityLoading = false;
    }
  }

  /** Prende, apaga o fuerza el filtro sin salir de la sección: es un solo ALTER TABLE. */
  async function applySwitch(change: PolicyChange) {
    if (!selected || !node) return;
    if (!(await confirmMutation(selected.profileId, "Se va a cambiar la seguridad de la tabla.")))
      return;
    securityError = null;
    try {
      await policyApply(selected.profileId, [change], node.database);
      await loadSecurity();
    } catch (error) {
      securityError = describeError(error);
    }
  }

  let privileges = $state<PrivilegeGrant[] | null>(null);
  let columnGrants = $state<ColumnGrant[]>([]);
  let defaultGrants = $state<DefaultGrant[]>([]);
  let privilegesError = $state<string | null>(null);
  let privilegesLoading = $state(false);
  /** Los argumentos de la función, que son parte de cómo se la nombra en un `GRANT`. */
  let routineArgs = $state("");

  async function loadPrivileges() {
    if (!selected || !node || !hasPrivileges) {
      privileges = null;
      columnGrants = [];
      defaultGrants = [];
      return;
    }
    privilegesLoading = true;
    privilegesError = null;
    try {
      const { profileId } = selected;
      const { oid, database } = node;

      if (isDatabase) {
        // Sin pasar la base: `pg_database` es un catálogo compartido, y abrir un pool contra la
        // base que se está mirando fallaría justamente cuando no se tiene CONNECT sobre ella.
        privileges = await databasePrivileges(profileId, node.label);
      } else if (oid === undefined) {
        privileges = null;
      } else if (isSchema) {
        privileges = await schemaPrivileges(profileId, oid, database);
        // Los privilegios por omisión no son de ningún objeto: se guardan por esquema, así que es
        // acá donde alguien los va a buscar.
        defaultGrants = (await defaultPrivileges(profileId, database)).filter(
          (grant) => grant.schema === node.label,
        );
      } else if (isRoutine) {
        privileges = await functionPrivileges(profileId, oid, database);
        routineArgs = await functionArgs(profileId, oid, database);
      } else {
        privileges = await relationPrivileges(profileId, oid, database);
        if (isTable) columnGrants = await columnPrivileges(profileId, oid, database);
      }
    } catch (error) {
      privilegesError = describeError(error);
    } finally {
      privilegesLoading = false;
    }
  }

  /** El objeto del que habla el diálogo de privilegios, con el vocabulario que le corresponde. */
  const privilegeSubject = $derived.by<PrivilegeSubject | null>(() => {
    if (!node) return null;
    if (isDatabase) return { on: "database", database: node.label };
    if (isSchema) return { on: "schema", schema: node.label };
    if (!node.schema) return null;
    if (isSequence) return { on: "sequence", schema: node.schema, sequence: node.label };
    if (isRoutine) {
      return {
        on: "function",
        schema: node.schema,
        name: node.label,
        args: routineArgs,
        procedure: isProcedure,
      };
    }
    // Las vistas y las materializadas comparten el vocabulario de una tabla.
    if (isTable || isView || isMaterializedView) {
      return { on: "table", schema: node.schema, table: node.label };
    }
    return null;
  });

  /** Una fila de `aclexplode` por privilegio: se agrupan por `grantee` para mostrar una sola línea. */
  const privilegeGroups = $derived.by<
    { grantee: string; privileges: string[]; grantable: boolean }[]
  >(() => {
    if (!privileges) return [];
    const byGrantee = new Map<
      string,
      { grantee: string; privileges: string[]; grantable: boolean }
    >();
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
  });

  /**
   * La clave de una fila de privilegios por columna. Va por JSON y no concatenando con un
   * separador: un nombre de columna puede tener cualquier cosa adentro, incluido el separador.
   */
  function pairKey(column: string, grantee: string): string {
    return JSON.stringify([column, grantee]);
  }

  /** Lo mismo que `privilegeGroups`, pero la fila es la combinación de columna y destinatario. */
  const columnGroups = $derived.by<{ column: string; grantee: string; privileges: string[] }[]>(
    () => {
      const byPair = new Map<string, { column: string; grantee: string; privileges: string[] }>();
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
    },
  );

  $effect(() => {
    // Depender de `node` (y no llamar directo) es lo que dispara de nuevo al cambiar de tabla.
    void node;
    loadShape();
    loadIndexes();
    loadConstraints();
    loadTriggers();
    loadSecurity();
    loadPrivileges();
  });

  // -------------------------------------------------------------------------
  // Secciones
  //
  // Una tabla tiene columnas, índices, restricciones, triggers, privilegios y DDL. Apilarlos en un
  // scroll obligaba a recorrer toda la página para saber si la tabla tiene un índice de más; en
  // pestañas, la cuenta se ve sin abrir nada y cada sección empieza arriba de todo.
  // -------------------------------------------------------------------------

  type SectionId =
    | "info"
    | "columns"
    | "indexes"
    | "constraints"
    | "triggers"
    | "security"
    | "privileges"
    | "mappings"
    | "ddl";

  const privilegeSection = $derived({
    id: "privileges" as const,
    label: "Privilegios",
    count: privileges ? privilegeGroups.length : null,
  });

  const sections = $derived.by<{ id: SectionId; label: string; count: number | null }[]>(() => {
    if (isServer) return [{ id: "info", label: "Propiedades", count: null }];
    if (isDatabase) {
      return [{ id: "info", label: "Propiedades", count: null }, privilegeSection];
    }
    if (isTable) {
      return [
        { id: "columns", label: "Columnas", count: shape?.columns.length ?? null },
        { id: "indexes", label: "Índices", count: indexes?.length ?? null },
        { id: "constraints", label: "Restricciones", count: constraints?.length ?? null },
        { id: "triggers", label: "Triggers", count: triggers?.length ?? null },
        { id: "security", label: "Seguridad por fila", count: security?.policies.length ?? null },
        privilegeSection,
        { id: "ddl", label: "DDL", count: null },
      ];
    }
    if (isForeignServer) {
      return [
        { id: "mappings", label: "Mapeos de usuario", count: mappings.length },
        { id: "ddl", label: "DDL", count: null },
      ];
    }
    if (hasPrivileges && hasDdl) return [privilegeSection, { id: "ddl", label: "DDL", count: null }];
    if (hasDdl) return [{ id: "ddl", label: "DDL", count: null }];
    return [];
  });

  let section = $state<SectionId>("columns");

  // Al cambiar de objeto se vuelve a la primera sección: quedarse en «Triggers» al pasar de una
  // tabla a un índice mostraría una pestaña que ese objeto no tiene. La lista se lee sin
  // registrarla como dependencia, porque sus contadores cambian cuando termina cada consulta y eso
  // devolvería al usuario a la primera pestaña mientras está mirando otra.
  $effect(() => {
    void selected;
    section = untrack(() => sections)[0]?.id ?? "ddl";
  });

  /** Busca la fila del árbol que tiene a `target` entre sus hijos, para refrescarla tras un cambio. */
  function parentOf(rows: Row[], target: Row): Row | null {
    for (const row of rows) {
      if (row.children?.includes(target)) return row;
      if (row.children) {
        const found = parentOf(row.children, target);
        if (found) return found;
      }
    }
    return null;
  }

  let newTable = $state(false);
  let newIndex = $state(false);
  let newConstraint = $state(false);
  let columnDialog = $state<{ column: TableColumn | null } | null>(null);
  let viewDialog = $state<{
    materialized: boolean;
    existing: { oid: number; name: string } | null;
  } | null>(null);
  let refreshTarget = $state<{ schema: string; name: string } | null>(null);
  let refreshConcurrently = $state(false);
  let refreshing = $state(false);
  let refreshError = $state<string | null>(null);
  let functionDialog = $state<{ sql: string; isEdit: boolean } | null>(null);
  let triggerDialog = $state<{ existing: TriggerInfo | null } | null>(null);
  let policyDialog = $state<{ existing: PolicyInfo | null } | null>(null);
  let roleDialog = $state<{ existing: RoleInfo | null } | null>(null);
  let extensionDialog = $state<{ existing: ExtensionInfo | null } | null>(null);
  let fdwDialog = $state<{ existing: FdwInfo | null } | null>(null);
  let foreignServerDialog = $state<{ existing: ServerInfo | null } | null>(null);
  let userMappingDialog = $state<{ existing: UserMapping | null } | null>(null);
  let mappings = $state<UserMapping[]>([]);
  let mappingsError = $state<string | null>(null);
  let mappingDrop = $state<{ user: string } | null>(null);
  let privilegeDialog = $state<{ existing: PrivilegeExisting | null } | null>(null);
  let backupDialog = $state(false);
  let restoreDialog = $state(false);
  let sequenceDialog = $state<{ existing: { oid: number; name: string } | null } | null>(null);
  let typeDialog = $state<{
    composite: boolean;
    existing: { oid: number; name: string } | null;
  } | null>(null);
  let domainDialog = $state<{ existing: { oid: number; name: string } | null } | null>(null);
  let schemaDialog = $state<{ existing: { name: string; owner: string } | null } | null>(null);
  let databaseDialog = $state<{ existing: string | null } | null>(null);
  let partitionDialog = $state<{ strategy: string } | null>(null);
  let commentDialog = $state<{ target: CommentTarget; label: string; current: string | null } | null>(
    null,
  );
  let exportDialog = $state(false);
  let importDialog = $state(false);
  let revokeTarget = $state<{ grantee: string; privileges: string[] } | null>(null);
  let revokeCascade = $state(false);
  let revoking = $state(false);
  let revokeError = $state<string | null>(null);
  let dropTarget = $state<
    | {
        kind:
          | "table"
          | "column"
          | "index"
          | "constraint"
          | "view"
          | "materializedView"
          | "trigger"
          | "policy"
          | "role"
          | "extension"
          | "foreignDataWrapper"
          | "foreignServer"
          | "sequence"
          | "type"
          | "domain"
          | "schema"
          | "database";
        label: string;
      }
    | { kind: "function"; schema: string; name: string; args: string; procedure: boolean }
    | null
  >(null);
  let dropCascade = $state(false);
  /** Solo se usa para índices: CASCADE y CONCURRENTLY son mutuamente excluyentes en Postgres. */
  let dropConcurrently = $state(false);
  /**
   * Solo para roles: un rol dueño de algo no se puede borrar, y Postgres no tiene
   * `DROP ROLE ... CASCADE`. Esto ofrece el camino que hay: pasarle sus objetos a otro y soltar los
   * privilegios que le hayan otorgado, todo en la misma transacción que el borrado.
   */
  let reassignFirst = $state(false);
  let reassignTo = $state("CURRENT_USER");
  let dropping = $state(false);
  let dropError = $state<string | null>(null);

  function afterTableCreated() {
    newTable = false;
    if (selected) explorer.reload(selected);
  }

  function afterColumnSaved() {
    columnDialog = null;
    loadShape();
  }

  function afterIndexCreated() {
    newIndex = false;
    loadIndexes();
  }

  function afterTriggerSaved() {
    triggerDialog = null;
    loadTriggers();
  }

  function afterPolicySaved() {
    policyDialog = null;
    loadSecurity();
  }

  /** Trae el rol tal como ya existe antes de abrir la edición: acá no hay nada precargado, a
   * diferencia de una columna, que ya viene con `shape`. */
  async function openEditRole() {
    if (!selected || !node?.oid) return;
    try {
      const info = await roleInfo(selected.profileId, node.oid, node.database);
      roleDialog = { existing: info };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  function afterRoleSaved() {
    const wasCreate = roleDialog !== null && roleDialog.existing === null;
    roleDialog = null;
    if (!selected) return;
    if (wasCreate) {
      explorer.reload(selected);
    } else {
      // Renombrar cambia lo que el árbol muestra: se recarga la carpeta "Roles" y se limpia la
      // selección, porque la fila que estaba elegida ya no coincide con el rol que quedó.
      const parent = parentOf(explorer.roots, selected);
      if (parent) explorer.reload(parent);
      explorer.selected = null;
    }
  }

  /** Trae la extensión tal como está antes de abrir la edición. */
  async function openEditExtension() {
    if (!selected || !node) return;
    try {
      const info = await extensionInfo(selected.profileId, node.label, node.database);
      extensionDialog = { existing: info };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  function afterExtensionSaved() {
    const wasInstall = extensionDialog !== null && extensionDialog.existing === null;
    extensionDialog = null;
    if (!selected) return;
    // Instalar suma un nodo a la carpeta; actualizar o cambiar de esquema cambia el detalle del nodo.
    // En ambos casos se recarga la carpeta "Extensiones" para reflejarlo.
    if (wasInstall) {
      explorer.reload(selected);
    } else {
      const parent = parentOf(explorer.roots, selected);
      if (parent) explorer.reload(parent);
      explorer.selected = null;
    }
  }

  // --- Datos externos (wrappers, servidores foráneos, mapeos) ---

  /** Al abrir el detalle de un servidor foráneo se listan sus mapeos de usuario. */
  $effect(() => {
    if (!isForeignServer || !selected || !node) {
      mappings = [];
      return;
    }
    const profileId = selected.profileId;
    const database = node.database;
    const server = node.label;
    let cancelled = false;
    mappingsError = null;
    userMappings(profileId, server, database)
      .then((result) => {
        if (!cancelled) mappings = result;
      })
      .catch((error) => {
        if (!cancelled) mappingsError = describeError(error);
      });
    return () => {
      cancelled = true;
    };
  });

  async function openEditFdw() {
    if (!selected || !node) return;
    try {
      fdwDialog = { existing: await fdwInfo(selected.profileId, node.label, node.database) };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  async function openEditForeignServer() {
    if (!selected || !node) return;
    try {
      foreignServerDialog = {
        existing: await foreignServerInfo(selected.profileId, node.label, node.database),
      };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  /** Recarga la carpeta del nodo tras crear/editar un wrapper o servidor, y limpia la selección. */
  function reloadFolderAndClear(wasCreate: boolean) {
    if (!selected) return;
    if (wasCreate) {
      explorer.reload(selected);
    } else {
      const parent = parentOf(explorer.roots, selected);
      if (parent) explorer.reload(parent);
      explorer.selected = null;
    }
  }

  function afterFdwSaved() {
    const wasCreate = fdwDialog?.existing == null;
    fdwDialog = null;
    reloadFolderAndClear(wasCreate);
  }

  function afterForeignServerSaved() {
    const wasCreate = foreignServerDialog?.existing == null;
    foreignServerDialog = null;
    reloadFolderAndClear(wasCreate);
  }

  function reloadMappings() {
    if (!selected || !node) return;
    userMappings(selected.profileId, node.label, node.database)
      .then((result) => (mappings = result))
      .catch((error) => (mappingsError = describeError(error)));
  }

  function afterMappingSaved() {
    userMappingDialog = null;
    reloadMappings();
  }

  async function confirmMappingDrop() {
    if (!mappingDrop || !selected || !node) return;
    try {
      await userMappingApply(
        selected.profileId,
        [{ kind: "drop", server: node.label, user: mappingDrop.user }],
        node.database,
      );
      mappingDrop = null;
      reloadMappings();
    } catch (error) {
      mappingsError = describeError(error);
    }
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

  function triggerSummary(t: TriggerInfo): string {
    const events = t.events.map((event) => EVENT_LABEL[event]).join(" OR ");
    return `${TIMING_LABEL[t.timing]} ${events} · ${t.level === "row" ? "ROW" : "STATEMENT"}`;
  }

  function afterPrivilegeSaved() {
    privilegeDialog = null;
    loadPrivileges();
  }

  async function confirmRevoke() {
    if (!revokeTarget || !selected || !node || !privilegeSubject) return;
    if (!(await confirmMutation(selected.profileId, "Se van a revocar privilegios."))) return;
    revoking = true;
    revokeError = null;
    try {
      await privilegeApply(
        selected.profileId,
        [
          {
            kind: "revoke",
            // Los privilegios salen de lo que devolvió el catálogo para este mismo objeto, así que
            // pertenecen a su vocabulario por construcción.
            target: { ...privilegeSubject, privileges: revokeTarget.privileges } as Grantable,
            grantee: revokeTarget.grantee,
            grantOptionOnly: false,
            cascade: revokeCascade,
          },
        ],
        node.database,
      );
      revokeTarget = null;
      revokeCascade = false;
      await loadPrivileges();
    } catch (error) {
      revokeError = describeError(error);
    } finally {
      revoking = false;
    }
  }

  function afterConstraintCreated() {
    newConstraint = false;
    loadConstraints();
    // Agregar una primary key o un unique puede cambiar si la grilla de datos de la tabla es
    // editable: se relee la forma para que ese estado no quede desactualizado en el panel.
    loadShape();
  }

  function closeDropDialog() {
    dropTarget = null;
    dropCascade = false;
    dropConcurrently = false;
    reassignFirst = false;
    reassignTo = "CURRENT_USER";
    dropError = null;
  }

  /** Un punto de partida mínimo: mejor que una pantalla en blanco, sin fingir saber qué necesita. */
  function functionSkeleton(schema: string, procedure: boolean): string {
    return procedure
      ? `CREATE PROCEDURE ${schema}.nombre()\nLANGUAGE plpgsql\nAS $$\nBEGIN\nEND;\n$$;`
      : `CREATE FUNCTION ${schema}.nombre()\nRETURNS void\nLANGUAGE plpgsql\nAS $$\nBEGIN\nEND;\n$$;`;
  }

  function afterFunctionSaved() {
    const wasCreate = functionDialog !== null && !functionDialog.isEdit;
    functionDialog = null;
    if (wasCreate) {
      if (selected) explorer.reload(selected);
    } else {
      refreshDdl();
    }
  }

  /** Busca la firma completa antes de confirmar: sin ella no se puede armar el DROP FUNCTION. */
  async function askDropFunction() {
    if (!selected || !node?.oid || !node.schema) return;
    try {
      const args = await functionArgs(selected.profileId, node.oid, node.database);
      dropTarget = {
        kind: "function",
        schema: node.schema,
        name: node.label,
        args,
        procedure: isProcedure,
      };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  /**
   * Un tipo y un dominio son los dos `pg_type` y el árbol no los distingue, así que hay que
   * preguntarle al servidor antes de saber qué formulario abrir.
   */
  async function openEditType() {
    if (!selected || !node?.oid) return;
    try {
      const info = await typeInfo(selected.profileId, node.oid, node.database);
      const existing = { oid: node.oid, name: node.label };
      if (info.kind === "domain") domainDialog = { existing };
      else typeDialog = { composite: info.kind === "composite", existing };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  /** La estrategia de partición la manda el servidor: decide qué límite pide el formulario. */
  async function openPartitionDialog() {
    if (!selected || !node?.oid) return;
    try {
      const info = await tablePartitions(selected.profileId, node.oid, node.database);
      partitionDialog = { strategy: info.strategy };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  /** Sobre qué objeto se comenta, a partir del nodo seleccionado. */
  function commentTargetOf(): CommentTarget | null {
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

  const commentTarget = $derived(commentTargetOf());

  /** Recarga el nodo carpeta que pasó a tener un objeto nuevo, o refresca el DDL tras editarlo. */
  function afterViewSaved() {
    const wasCreate = viewDialog !== null && viewDialog.existing === null;
    viewDialog = null;
    if (wasCreate) {
      if (selected) explorer.reload(selected);
    } else {
      refreshDdl();
    }
  }

  async function confirmRefresh() {
    if (!refreshTarget || !selected) return;
    if (!(await confirmMutation(selected.profileId, "Se va a recalcular la vista materializada.")))
      return;
    refreshing = true;
    refreshError = null;
    try {
      await viewApply(
        selected.profileId,
        [
          {
            kind: "refreshMaterializedView",
            schema: refreshTarget.schema,
            name: refreshTarget.name,
            concurrently: refreshConcurrently,
          },
        ],
        node?.database,
      );
      refreshTarget = null;
      refreshConcurrently = false;
    } catch (error) {
      refreshError = describeError(error);
    } finally {
      refreshing = false;
    }
  }

  async function confirmDrop() {
    if (!dropTarget || !selected || !node) return;
    if (!(await confirmMutation(selected.profileId, "Se va a eliminar un objeto del servidor.")))
      return;
    dropping = true;
    dropError = null;
    try {
      switch (dropTarget.kind) {
        case "table":
          await ddlApply(
            selected.profileId,
            [{ kind: "dropTable", schema: shape!.schema, name: shape!.name, cascade: dropCascade }],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "column":
          await ddlApply(
            selected.profileId,
            [
              {
                kind: "dropColumn",
                schema: shape!.schema,
                table: shape!.name,
                column: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          await loadShape();
          break;
        case "index":
          await indexDrop(
            selected.profileId,
            shape!.schema,
            dropTarget.label,
            dropCascade,
            dropConcurrently,
            node.database,
          );
          await loadIndexes();
          break;
        case "constraint":
          await ddlApply(
            selected.profileId,
            [
              {
                kind: "dropConstraint",
                schema: shape!.schema,
                table: shape!.name,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          await loadConstraints();
          await loadShape();
          break;
        case "trigger":
          await triggerApply(
            selected.profileId,
            [
              {
                kind: "dropTrigger",
                schema: shape!.schema,
                table: shape!.name,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          await loadTriggers();
          break;
        case "view":
          await viewApply(
            selected.profileId,
            [
              {
                kind: "dropView",
                schema: node.schema!,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "materializedView":
          await viewApply(
            selected.profileId,
            [
              {
                kind: "dropMaterializedView",
                schema: node.schema!,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "function":
          await functionDrop(
            selected.profileId,
            dropTarget.schema,
            dropTarget.name,
            dropTarget.args,
            dropTarget.procedure,
            dropCascade,
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "policy":
          await policyApply(
            selected.profileId,
            [
              {
                kind: "dropPolicy",
                schema: shape!.schema,
                table: shape!.name,
                name: dropTarget.label,
              },
            ],
            node.database,
          );
          await loadSecurity();
          break;
        case "role":
          await roleApply(
            selected.profileId,
            [
              ...(reassignFirst
                ? ([
                    {
                      kind: "reassignOwned",
                      from: dropTarget.label,
                      to: reassignTo.trim() || "CURRENT_USER",
                    },
                    { kind: "dropOwned", role: dropTarget.label, cascade: false },
                  ] as RoleChange[])
                : []),
              { kind: "dropRole", name: dropTarget.label },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "extension":
          await extensionApply(
            selected.profileId,
            [{ kind: "drop", name: dropTarget.label, cascade: dropCascade }],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "sequence":
          await sequenceApply(
            selected.profileId,
            [
              {
                kind: "dropSequence",
                schema: node.schema!,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "type":
          await typeApply(
            selected.profileId,
            [
              {
                kind: "dropType",
                schema: node.schema!,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "domain":
          await domainApply(
            selected.profileId,
            [
              {
                kind: "dropDomain",
                schema: node.schema!,
                name: dropTarget.label,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "schema":
          await schemaApply(
            selected.profileId,
            [
              {
                kind: "dropSchema",
                name: dropTarget.label,
                ifExists: false,
                cascade: dropCascade,
              },
            ],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "database":
          // `force` echa a las sesiones conectadas: sin eso, una base con alguien adentro no se
          // borra y el error no dice quién es.
          await databaseApply(selected.profileId, [
            { kind: "dropDatabase", name: dropTarget.label, ifExists: false, force: dropCascade },
          ]);
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "foreignDataWrapper":
          await fdwApply(
            selected.profileId,
            [{ kind: "drop", name: dropTarget.label, cascade: dropCascade }],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
        case "foreignServer":
          await foreignServerApply(
            selected.profileId,
            [{ kind: "drop", name: dropTarget.label, cascade: dropCascade }],
            node.database,
          );
          {
            const parent = parentOf(explorer.roots, selected);
            if (parent) await explorer.reload(parent);
          }
          explorer.selected = null;
          break;
      }
      closeDropDialog();
    } catch (error) {
      dropError = describeError(error);
    } finally {
      dropping = false;
    }
  }

  const dropQuestion = $derived.by(() => {
    if (!dropTarget) return "";
    switch (dropTarget.kind) {
      case "table":
        return `¿Eliminar la tabla ${dropTarget.label}? Se pierden sus datos.`;
      case "column":
        return `¿Eliminar la columna ${dropTarget.label}? Se pierden sus valores en todas las filas.`;
      case "index":
        return `¿Eliminar el índice ${dropTarget.label}?`;
      case "constraint":
        return `¿Eliminar la restricción ${dropTarget.label}?`;
      case "trigger":
        return `¿Eliminar el trigger ${dropTarget.label}?`;
      case "view":
        return `¿Eliminar la vista ${dropTarget.label}?`;
      case "materializedView":
        return `¿Eliminar la vista materializada ${dropTarget.label}? Se pierden los datos guardados.`;
      case "function":
        return `¿Eliminar ${dropTarget.procedure ? "el procedimiento" : "la función"} ${dropTarget.name}(${dropTarget.args})?`;
      case "policy":
        return `¿Eliminar la política ${dropTarget.label}? Si es la única que dejaba ver filas, la tabla queda sin nada visible.`;
      case "role":
        return `¿Eliminar el rol ${dropTarget.label}?`;
      case "extension":
        return `¿Quitar la extensión ${dropTarget.label} de la base?`;
      case "foreignDataWrapper":
        return `¿Eliminar el wrapper foráneo ${dropTarget.label}?`;
      case "foreignServer":
        return `¿Eliminar el servidor foráneo ${dropTarget.label}? Se pierden sus mapeos de usuario.`;
      case "sequence":
        return `¿Eliminar la secuencia ${dropTarget.label}?`;
      case "type":
        return `¿Eliminar el tipo ${dropTarget.label}? Las columnas que lo usan lo necesitan.`;
      case "domain":
        return `¿Eliminar el dominio ${dropTarget.label}? Las columnas que lo usan lo necesitan.`;
      case "schema":
        return `¿Eliminar el esquema ${dropTarget.label}? Sin CASCADE falla si tiene algo adentro.`;
      case "database":
        return `¿Eliminar la base ${dropTarget.label}? Se pierde todo lo que tiene adentro.`;
    }
  });

  /** Lo que no cabe en el encabezado: los datos de la conexión, o los de una base. */
  const properties = $derived.by<{ label: string; value: string; bad?: boolean }[]>(() => {
    if (!isServer) {
      if (!node) return [];
      const rows: { label: string; value: string; bad?: boolean }[] = [
        { label: "Base de datos", value: node.database },
      ];
      if (node.schema) rows.push({ label: "Esquema", value: node.schema });
      if (node.oid) rows.push({ label: "OID", value: String(node.oid) });
      return rows;
    }
    if (!profile) return [];
    const rows: { label: string; value: string; bad?: boolean }[] = [
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
  });

  /** La ruta del objeto: base / esquema. Contesta «¿de dónde salió esto?» sin volver al árbol. */
  const path = $derived.by(() => {
    if (isServer) return profile ? `${profile.host}:${profile.port}` : "";
    if (!node) return "";
    return [node.database, node.schema].filter(Boolean).join(" / ");
  });
</script>

<div class="flex h-full flex-col">
  {#if !selected}
    <Empty
      icon="compass"
      title="Nada seleccionado"
      hint="Elegí un servidor, una base o un objeto del árbol de la izquierda para ver su detalle, su DDL y las acciones que admite."
    />
  {:else}
    <header class="divider-b px-5 py-3">
      <div class="flex items-center gap-2.5">
        <div
          class="grid size-9 shrink-0 place-items-center rounded-lg bg-zinc-100 dark:bg-zinc-800
                 {look.tone}"
        >
          <Icon name={look.icon} size={18} />
        </div>

        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <h2 class="truncate text-base font-medium">{selected.label}</h2>
            <span class="tag tag-neutral shrink-0">
              {isGroup ? "Carpeta de conexiones" : kindLabel(node?.kind ?? null)}
            </span>
            {#if isServer}
              <span class="tag shrink-0 {selected.connected ? 'tag-ok' : 'tag-neutral'}">
                {selected.connected ? "conectado" : "sin conectar"}
              </span>
            {/if}
          </div>
          {#if path}
            <p class="truncate text-xs muted">{path}</p>
          {/if}
        </div>

        <div class="ml-auto flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          {#if isTablesFolder && node?.schema}
            <button class="btn btn-primary" onclick={() => (newTable = true)}>
              <Icon name="plus" size={12} />
              Nueva tabla
            </button>
          {/if}

          {#if isViewsFolder && node?.schema}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() => (viewDialog = { materialized: false, existing: null })}
            >
              <Icon name="plus" size={12} />
              Nueva vista
            </button>
          {/if}

          {#if isMatViewsFolder && node?.schema}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() => (viewDialog = { materialized: true, existing: null })}
            >
              <Icon name="plus" size={12} />
              Nueva vista materializada
            </button>
          {/if}

          {#if isFunctionsFolder && node?.schema}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() =>
                (functionDialog = { sql: functionSkeleton(node!.schema!, false), isEdit: false })}
            >
              <Icon name="plus" size={12} />
              Nueva función
            </button>
          {/if}

          {#if isProceduresFolder && node?.schema}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() =>
                (functionDialog = { sql: functionSkeleton(node!.schema!, true), isEdit: false })}
            >
              <Icon name="plus" size={12} />
              Nuevo procedimiento
            </button>
          {/if}

          {#if isRolesFolder}
            <button class="btn btn-primary" {...blocked} onclick={() => (roleDialog = { existing: null })}>
              <Icon name="plus" size={12} />
              Nuevo rol
            </button>
          {/if}

          {#if isExtensionsFolder}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() => (extensionDialog = { existing: null })}
            >
              <Icon name="plus" size={12} />
              Instalar extensión
            </button>
          {/if}

          {#if isFdwsFolder}
            <button class="btn btn-primary" {...blocked} onclick={() => (fdwDialog = { existing: null })}>
              <Icon name="plus" size={12} />
              Nuevo wrapper
            </button>
          {/if}

          {#if isForeignServersFolder}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() => (foreignServerDialog = { existing: null })}
            >
              <Icon name="plus" size={12} />
              Nuevo servidor foráneo
            </button>
          {/if}

          {#if isSequencesFolder && node?.schema}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() => (sequenceDialog = { existing: null })}
            >
              <Icon name="plus" size={12} />
              Nueva secuencia
            </button>
          {/if}

          {#if isTypesFolder && node?.schema}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() => (typeDialog = { composite: false, existing: null })}
            >
              <Icon name="plus" size={12} />
              Nueva enumeración
            </button>
            <button
              class="btn"
              {...blocked}
              onclick={() => (typeDialog = { composite: true, existing: null })}
            >
              <Icon name="plus" size={12} />
              Nuevo compuesto
            </button>
            <button class="btn" {...blocked} onclick={() => (domainDialog = { existing: null })}>
              <Icon name="plus" size={12} />
              Nuevo dominio
            </button>
          {/if}

          {#if isSchemasFolder}
            <button
              class="btn btn-primary"
              {...blocked}
              onclick={() => (schemaDialog = { existing: null })}
            >
              <Icon name="plus" size={12} />
              Nuevo esquema
            </button>
          {/if}

          {#if isPartitionedTable && node?.oid}
            <button
              class="btn"
              title="Crea o engancha una partición de esta tabla"
              {...blocked}
              onclick={openPartitionDialog}
            >
              <Icon name="plus" size={12} />
              Nueva partición
            </button>
          {/if}

          {#if dataTarget !== null && queryTarget}
            <button
              class="btn btn-primary"
              title={`Abre los datos de ${selected.label}`}
              onclick={() =>
                ondata(selected.profileId, queryTarget.database, queryTarget.title, dataTarget)}
            >
              <Icon name="table" size={12} />
              Datos
            </button>
          {/if}

          {#if queryTarget}
            <button
              class="btn"
              title={`Abre una consulta contra ${queryTarget.database}`}
              onclick={() => onquery(selected.profileId, queryTarget.database, queryTarget.title)}
            >
              <Icon name="sql" size={12} />
              Consulta
            </button>
          {/if}

          {#if isSchema && node}
            <button
              class="btn"
              title={`Dibuja las tablas de ${node.label} y sus claves foráneas`}
              onclick={() => onerd(selected.profileId, node.database, node.label)}
            >
              <Icon name="diagram" size={12} />
              Diagrama
            </button>
          {/if}

          {#if isTable && node}
            <button
              class="btn"
              title={`Exporta ${node.label} a un archivo con COPY`}
              onclick={() => (exportDialog = true)}
            >
              <Icon name="download" size={12} />
              Exportar
            </button>
            <button
              class="btn"
              title={`Importa un archivo a ${node.label} con COPY`}
              {...blocked}
              onclick={() => (importDialog = true)}
            >
              <Icon name="upload" size={12} />
              Importar
            </button>
          {/if}

          {#if isDatabase && node}
            <button
              class="btn"
              title={`Hace un backup de ${node.label} con pg_dump`}
              onclick={() => (backupDialog = true)}
            >
              <Icon name="download" size={12} />
              Backup
            </button>
            <button
              class="btn"
              title={`Restaura un backup sobre ${node.label} con pg_restore`}
              {...blocked}
              onclick={() => (restoreDialog = true)}
            >
              <Icon name="upload" size={12} />
              Restaurar
            </button>
          {/if}

          {#if isView && node}
            <button
              class="btn"
              {...blocked}
              onclick={() =>
                (viewDialog = {
                  materialized: false,
                  existing: { oid: node!.oid!, name: node!.label },
                })}
            >
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isMaterializedView && node}
            <button
              class="btn"
              {...blocked}
              onclick={() =>
                (viewDialog = {
                  materialized: true,
                  existing: { oid: node!.oid!, name: node!.label },
                })}
            >
              <Icon name="edit" size={12} />
              Editar
            </button>
            <button
              class="btn"
              title="Vuelve a calcular los datos guardados de la vista"
              {...blocked}
              onclick={() => (refreshTarget = { schema: node!.schema!, name: node!.label })}
            >
              <Icon name="refresh" size={12} />
              Refrescar
            </button>
          {/if}

          {#if (isFunction || isProcedure) && ddl}
            <button class="btn" {...blocked} onclick={() => (functionDialog = { sql: ddl!.sql, isEdit: true })}>
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isRole}
            <button class="btn" onclick={openEditRole}>
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isExtension}
            <button class="btn" onclick={openEditExtension}>
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isFdw}
            <button class="btn" onclick={openEditFdw}>
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isForeignServer}
            <button class="btn" onclick={openEditForeignServer}>
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isSequence && node?.oid}
            <button
              class="btn"
              {...blocked}
              onclick={() => (sequenceDialog = { existing: { oid: node!.oid!, name: node!.label } })}
            >
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isType && node?.oid}
            <button class="btn" {...blocked} onclick={openEditType}>
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isSchema && node}
            <button
              class="btn"
              {...blocked}
              onclick={() =>
                (schemaDialog = { existing: { name: node!.label, owner: node!.detail ?? "" } })}
            >
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          {#if isDatabase && node}
            <button class="btn" {...blocked} onclick={() => (databaseDialog = { existing: node!.label })}>
              <Icon name="edit" size={12} />
              Editar
            </button>
            <button class="btn" {...blocked} onclick={() => (databaseDialog = { existing: null })}>
              <Icon name="plus" size={12} />
              Nueva base
            </button>
          {/if}

          {#if commentTarget}
            <button
              class="btn"
              title="Documenta el objeto adentro de la propia base"
              {...blocked}
              onclick={() =>
                (commentDialog = {
                  target: commentTarget!,
                  label: selected.label,
                  current: node?.comment ?? null,
                })}
            >
              <Icon name="edit" size={12} />
              Comentario
            </button>
          {/if}

          {#if isGroup}
            <button class="btn btn-primary" onclick={() => ongroup(selected.group!)}>
              <Icon name="edit" size={12} />
              Renombrar
            </button>
          {/if}

          {#if isServer}
            {#if selected.connected}
              <button class="btn" onclick={() => explorer.disconnect(selected.profileId)}>
                Desconectar
              </button>
            {:else}
              <button class="btn btn-primary" onclick={() => onconnect(selected.profileId)}>
                Conectar
              </button>
            {/if}
            <button class="btn" onclick={() => onedit(selected.profileId)}>
              <Icon name="edit" size={12} />
              Editar
            </button>
          {/if}

          <!-- Lo que borra va último y en rojo: se llega a ello después de todo lo demás. -->
          {#if isTable && shape}
            <button
              class="btn btn-danger-ghost"
              title="Eliminar la tabla"
              {...blocked}
              onclick={() => (dropTarget = { kind: "table", label: shape!.name })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isView && node}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "view", label: node!.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isMaterializedView && node}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "materializedView", label: node!.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if (isFunction || isProcedure) && ddl}
            <button class="btn btn-danger-ghost" {...blocked} onclick={askDropFunction}>
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isSequence && node}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "sequence", label: node!.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isType && node}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "type", label: node!.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isSchema && node}
            <button
              class="btn btn-danger-ghost"
              title="Sin CASCADE falla si el esquema tiene algo adentro"
              {...blocked}
              onclick={() => (dropTarget = { kind: "schema", label: node!.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isDatabase && node}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "database", label: node!.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isRole}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "role", label: selected.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isExtension}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "extension", label: selected.label })}
            >
              <Icon name="trash" size={12} />
              Quitar
            </button>
          {/if}

          {#if isFdw}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "foreignDataWrapper", label: selected.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isForeignServer}
            <button
              class="btn btn-danger-ghost"
              {...blocked}
              onclick={() => (dropTarget = { kind: "foreignServer", label: selected.label })}
            >
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}

          {#if isServer}
            <button class="btn btn-danger-ghost" onclick={() => ondelete(selected.profileId)}>
              <Icon name="trash" size={12} />
              Eliminar
            </button>
          {/if}
        </div>
      </div>

      {#if selected.comment}
        <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">{selected.comment}</p>
      {/if}
    </header>

    {#if isGroup}
      <!--
        Una carpeta no tiene catálogo que mostrar: lo único que contiene son conexiones, así que el
        panel es la lista de esas conexiones con lo que se puede hacer con cada una.
      -->
      <div class="min-h-0 flex-1 overflow-auto p-4">
        <div class="card overflow-hidden">
          <div class="card-head">
            <span class="card-title">Servidores de la carpeta</span>
            <span class="ml-auto text-xs muted">
              Arrastrá un servidor del árbol para meterlo o sacarlo
            </span>
          </div>

          <table class="list-table">
            <tbody>
              {#each groupServers as server (server.profileId)}
                <tr class="group">
                  <td class="w-px whitespace-nowrap">
                    <span class="flex items-center gap-1.5">
                      <span class="dot {server.connected ? 'dot-on' : 'dot-off'}"></span>
                      <span class="font-medium">{server.label}</span>
                    </span>
                  </td>
                  <td class="text-xs muted">{server.detail}</td>
                  <td class="w-40">
                    <div class="row-actions">
                      {#if server.connected}
                        <button
                          class="btn btn-sm"
                          onclick={() => explorer.disconnect(server.profileId)}
                        >
                          Desconectar
                        </button>
                      {:else}
                        <button class="btn btn-sm" onclick={() => onconnect(server.profileId)}>
                          Conectar
                        </button>
                      {/if}
                      <button
                        class="btn btn-ghost btn-icon size-6"
                        title="Editar el servidor"
                        aria-label="Editar el servidor"
                        onclick={() => onedit(server.profileId)}
                      >
                        <Icon name="edit" size={12} />
                      </button>
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {:else if isServer && !selected.connected}
      <Empty
        icon="server"
        title="El servidor está sin conectar"
        hint="Conectate para explorar sus bases, esquemas y objetos."
      >
        <button class="btn btn-primary" onclick={() => onconnect(selected.profileId)}>
          Conectar
        </button>
      </Empty>
    {:else if isFolder && sections.length === 0}
      <Empty
        icon="folder"
        title="Una carpeta del árbol"
        hint="Agrupa objetos del mismo tipo; no tiene un DDL propio. Abrila en el árbol para ver lo que contiene."
      />
    {:else if sections.length === 0}
      <Empty icon="info" title="Sin detalle" hint="Este nodo no tiene propiedades para mostrar." />
    {:else}
      {#if sections.length > 1}
        <div class="divider-b flex items-center gap-2 px-4 py-1.5">
          <div class="seg" role="tablist">
            {#each sections as item (item.id)}
              <button
                class="seg-item"
                role="tab"
                aria-selected={section === item.id}
                onclick={() => (section = item.id)}
              >
                {item.label}
                {#if item.count !== null}
                  <span class="seg-count">{item.count}</span>
                {/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <div class="min-h-0 flex-1 overflow-auto p-4">
        {#if section === "info"}
          <div class="card overflow-hidden">
            <div class="card-head"><span class="card-title">Propiedades</span></div>
            <table class="list-table">
              <tbody>
                {#each properties as row (row.label)}
                  <tr>
                    <td class="w-56 muted">{row.label}</td>
                    <td class={row.bad ? "text-amber-700 dark:text-amber-400" : ""}>{row.value}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else if section === "columns"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">Columnas</span>
              {#if shape}
                <button
                  class="btn btn-sm ml-auto"
                  {...blocked}
                  onclick={() => (columnDialog = { column: null })}
                >
                  <Icon name="plus" size={11} />
                  Columna
                </button>
              {/if}
            </div>

            {#if shapeLoading}
              {@render pending("Leyendo columnas…")}
            {:else if shapeError}
              <Alert tone="bad" box class="m-3">{shapeError}</Alert>
            {:else if shape}
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Nombre</th>
                    <th class="w-px whitespace-nowrap">Tipo</th>
                    <th class="w-full">Valor por omisión</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each shape.columns as column (column.name)}
                    <tr class="group">
                      <td class="w-px font-medium whitespace-nowrap">
                        {column.name}
                        {#if column.notNull}
                          <span class="tag tag-neutral ml-1">NOT NULL</span>
                        {/if}
                      </td>
                      <td class="w-px font-mono text-xs whitespace-nowrap muted">
                        {column.typeName}
                      </td>
                      <td class="max-w-0 truncate text-xs muted">
                        {column.default ?? (column.generated ? "generada por el servidor" : "—")}
                      </td>
                      <td class="w-28">
                        <div class="row-actions">
                          {#if !column.generated}
                            <button
                              class="btn btn-ghost btn-icon size-6"
                              title="Editar la columna"
                              aria-label="Editar la columna"
                              {...blocked}
                              onclick={() => (columnDialog = { column })}
                            >
                              <Icon name="edit" size={12} />
                            </button>
                          {/if}
                          <button
                            class="btn btn-danger-ghost btn-icon size-6"
                            title="Eliminar la columna"
                            aria-label="Eliminar la columna"
                            {...blocked}
                            onclick={() => (dropTarget = { kind: "column", label: column.name })}
                          >
                            <Icon name="trash" size={12} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        {:else if section === "indexes"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">Índices</span>
              {#if shape}
                <button class="btn btn-sm ml-auto" {...blocked} onclick={() => (newIndex = true)}>
                  <Icon name="plus" size={11} />
                  Índice
                </button>
              {/if}
            </div>

            {#if indexesLoading}
              {@render pending("Leyendo índices…")}
            {:else if indexesError}
              <Alert tone="bad" box class="m-3">{indexesError}</Alert>
            {:else if indexes && indexes.length === 0}
              {@render nothing("No tiene índices propios.")}
            {:else if indexes}
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Nombre</th>
                    <th class="w-full">Definición</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each indexes as index (index.oid)}
                    <tr class="group">
                      <td class="w-px font-medium whitespace-nowrap">
                        {index.name}
                        {#if index.primary}
                          <span class="tag tag-info ml-1">primario</span>
                        {:else if index.unique}
                          <span class="tag tag-info ml-1">único</span>
                        {/if}
                        {#if !index.valid}
                          <span class="tag tag-bad ml-1">inválido</span>
                        {/if}
                      </td>
                      <td class="max-w-0 truncate font-mono text-xs muted" title={index.definition}>
                        {index.definition}
                      </td>
                      <td class="w-16">
                        <div class="row-actions">
                          <button
                            class="btn btn-danger-ghost btn-icon size-6"
                            title="Eliminar el índice"
                            aria-label="Eliminar el índice"
                            {...blocked}
                            onclick={() => (dropTarget = { kind: "index", label: index.name })}
                          >
                            <Icon name="trash" size={12} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        {:else if section === "constraints"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">Restricciones</span>
              {#if shape}
                <button class="btn btn-sm ml-auto" {...blocked} onclick={() => (newConstraint = true)}>
                  <Icon name="plus" size={11} />
                  Restricción
                </button>
              {/if}
            </div>

            {#if constraintsLoading}
              {@render pending("Leyendo restricciones…")}
            {:else if constraintsError}
              <Alert tone="bad" box class="m-3">{constraintsError}</Alert>
            {:else if constraints && constraints.length === 0}
              {@render nothing("No tiene restricciones propias.")}
            {:else if constraints}
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Nombre</th>
                    <th class="w-full">Definición</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each constraints as constraint (constraint.oid)}
                    <tr class="group">
                      <td class="w-px font-medium whitespace-nowrap">
                        {constraint.name}
                        <span class="tag tag-neutral ml-1">{constraint.kind}</span>
                      </td>
                      <td
                        class="max-w-0 truncate font-mono text-xs muted"
                        title={constraint.definition}
                      >
                        {constraint.definition}
                      </td>
                      <td class="w-16">
                        <div class="row-actions">
                          <button
                            class="btn btn-danger-ghost btn-icon size-6"
                            title="Eliminar la restricción"
                            aria-label="Eliminar la restricción"
                            {...blocked}
                            onclick={() =>
                              (dropTarget = { kind: "constraint", label: constraint.name })}
                          >
                            <Icon name="trash" size={12} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        {:else if section === "triggers"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">Triggers</span>
              {#if shape}
                <button
                  class="btn btn-sm ml-auto"
                  {...blocked}
                  onclick={() => (triggerDialog = { existing: null })}
                >
                  <Icon name="plus" size={11} />
                  Trigger
                </button>
              {/if}
            </div>

            {#if triggersLoading}
              {@render pending("Leyendo triggers…")}
            {:else if triggersError}
              <Alert tone="bad" box class="m-3">{triggersError}</Alert>
            {:else if triggers && triggers.length === 0}
              {@render nothing("No tiene triggers propios.")}
            {:else if triggers}
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Nombre</th>
                    <th class="w-px whitespace-nowrap">Cuándo</th>
                    <th class="w-full">Ejecuta</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each triggers as trigger (trigger.oid)}
                    <tr class="group">
                      <td class="w-px font-medium whitespace-nowrap">{trigger.name}</td>
                      <td class="w-px text-xs whitespace-nowrap muted">{triggerSummary(trigger)}</td>
                      <td class="max-w-0 truncate font-mono text-xs muted">
                        {trigger.functionSchema}.{trigger.functionName}()
                      </td>
                      <td class="w-24">
                        <div class="row-actions">
                          <button
                            class="btn btn-ghost btn-icon size-6"
                            title="Editar el trigger"
                            aria-label="Editar el trigger"
                            {...blocked}
                            onclick={() => (triggerDialog = { existing: trigger })}
                          >
                            <Icon name="edit" size={12} />
                          </button>
                          <button
                            class="btn btn-danger-ghost btn-icon size-6"
                            title="Eliminar el trigger"
                            aria-label="Eliminar el trigger"
                            {...blocked}
                            onclick={() => (dropTarget = { kind: "trigger", label: trigger.name })}
                          >
                            <Icon name="trash" size={12} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        {:else if section === "security"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">Seguridad por fila</span>
              {#if shape}
                <button
                  class="btn btn-sm ml-auto"
                  {...blocked}
                  onclick={() => (policyDialog = { existing: null })}
                >
                  <Icon name="plus" size={11} />
                  Política
                </button>
              {/if}
            </div>

            {#if securityLoading}
              {@render pending("Leyendo las políticas…")}
            {:else if securityError}
              <Alert tone="bad" box class="m-3">{securityError}</Alert>
            {:else if security}
              <div class="flex flex-col gap-2 border-b border-zinc-200 p-3 dark:border-zinc-800">
                <label class="check">
                  <input
                    type="checkbox"
                    checked={security.enabled}
                    onchange={() =>
                      applySwitch({
                        kind: "setRowSecurity",
                        schema: shape!.schema,
                        table: shape!.name,
                        enabled: !security!.enabled,
                      })}
                  />
                  Filtrar las filas según las políticas
                </label>
                <label class="check">
                  <input
                    type="checkbox"
                    checked={security.forced}
                    disabled={!security.enabled}
                    onchange={() =>
                      applySwitch({
                        kind: "setForceRowSecurity",
                        schema: shape!.schema,
                        table: shape!.name,
                        forced: !security!.forced,
                      })}
                  />
                  Aplicarlo también al dueño de la tabla
                </label>
              </div>

              <!-- Las tres combinaciones que engañan: filtro sin políticas no deja pasar nada,
                   políticas sin filtro no hacen nada, y el dueño se saltea todo si no se fuerza. -->
              {#if security.enabled && security.policies.length === 0}
                <Alert tone="warn" box class="m-3">
                  El filtro está activo y no hay ninguna política: la tabla no devuelve ninguna fila.
                </Alert>
              {:else if !security.enabled && security.policies.length > 0}
                <Alert tone="warn" box class="m-3">
                  Hay políticas definidas pero el filtro está apagado: no se aplica ninguna.
                </Alert>
              {:else if security.enabled && !security.forced}
                <Alert tone="ok" box class="m-3">
                  El dueño de la tabla se saltea el filtro. Para probar las políticas hay que
                  conectarse con otro rol, o marcar la segunda casilla.
                </Alert>
              {/if}

              {#if security.policies.length === 0}
                {@render nothing("No tiene políticas.")}
              {:else}
                <table class="list-table">
                  <thead>
                    <tr>
                      <th class="w-px whitespace-nowrap">Nombre</th>
                      <th class="w-px whitespace-nowrap">Comando</th>
                      <th class="w-px whitespace-nowrap">Roles</th>
                      <th class="w-full">Condición</th>
                      <th></th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each security.policies as policy (policy.oid)}
                      <tr class="group">
                        <td class="w-px font-medium whitespace-nowrap">{policy.name}</td>
                        <td class="w-px whitespace-nowrap">
                          <span class="tag tag-neutral font-mono">
                            {policy.command.toUpperCase()}
                          </span>
                          {#if policy.kind === "restrictive"}
                            <span class="tag tag-info">restrictiva</span>
                          {/if}
                        </td>
                        <td class="w-px text-xs whitespace-nowrap muted">
                          {policy.roles.length === 0 ? "PUBLIC" : policy.roles.join(", ")}
                        </td>
                        <td class="max-w-0 truncate font-mono text-xs muted">
                          {policy.using ?? ""}{policy.using && policy.check ? " · " : ""}{policy.check
                            ? `CHECK ${policy.check}`
                            : ""}
                        </td>
                        <td class="w-24">
                          <div class="row-actions">
                            <button
                              class="btn btn-ghost btn-icon size-6"
                              title="Editar la política"
                              aria-label="Editar la política"
                              {...blocked}
                              onclick={() => (policyDialog = { existing: policy })}
                            >
                              <Icon name="edit" size={12} />
                            </button>
                            <button
                              class="btn btn-danger-ghost btn-icon size-6"
                              title="Eliminar la política"
                              aria-label="Eliminar la política"
                              {...blocked}
                              onclick={() => (dropTarget = { kind: "policy", label: policy.name })}
                            >
                              <Icon name="trash" size={12} />
                            </button>
                          </div>
                        </td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {/if}
            {/if}
          </div>
        {:else if section === "privileges"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">Privilegios</span>
              <button
                class="btn btn-sm ml-auto"
                {...blocked}
                onclick={() => (privilegeDialog = { existing: null })}
              >
                <Icon name="plus" size={11} />
                Privilegio
              </button>
            </div>

            {#if privilegesLoading}
              {@render pending("Leyendo privilegios…")}
            {:else if privilegesError}
              <Alert tone="bad" box class="m-3">{privilegesError}</Alert>
            {:else if privilegeGroups.length === 0}
              {@render nothing(
                "Nadie tiene privilegios propios: rige el default (el dueño puede todo).",
              )}
            {:else}
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Rol</th>
                    <th class="w-full">Privilegios</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each privilegeGroups as group (group.grantee)}
                    <tr class="group">
                      <td class="w-px font-medium whitespace-nowrap">{group.grantee}</td>
                      <td>
                        <span class="flex flex-wrap gap-1">
                          <!-- Un mismo privilegio puede venir dos veces con otorgantes distintos:
                               la clave es la posición y no el nombre. -->
                          {#each group.privileges as privilege, position (position)}
                            <span class="tag tag-neutral font-mono">{privilege.toUpperCase()}</span>
                          {/each}
                          {#if group.grantable}
                            <span class="tag tag-info">con GRANT OPTION</span>
                          {/if}
                        </span>
                      </td>
                      <td class="w-24">
                        <div class="row-actions">
                          <button
                            class="btn btn-ghost btn-icon size-6"
                            title="Editar los privilegios"
                            aria-label="Editar los privilegios"
                            {...blocked}
                            onclick={() =>
                              (privilegeDialog = {
                                existing: {
                                  grantee: group.grantee,
                                  privileges: group.privileges,
                                  grantable: group.grantable,
                                  columns: columnGrants.filter(
                                    (grant) => grant.grantee === group.grantee,
                                  ),
                                },
                              })}
                          >
                            <Icon name="edit" size={12} />
                          </button>
                          <button
                            class="btn btn-danger-ghost btn-icon size-6"
                            title="Revocar todo"
                            aria-label="Revocar todo"
                            {...blocked}
                            onclick={() =>
                              (revokeTarget = {
                                grantee: group.grantee,
                                privileges: group.privileges,
                              })}
                          >
                            <Icon name="trash" size={12} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>

          {#if columnGroups.length > 0}
            <div class="card mt-3 overflow-hidden">
              <div class="card-head">
                <span class="card-title">Acotados a columnas</span>
                <span class="seg-count">{columnGroups.length}</span>
              </div>
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Columna</th>
                    <th class="w-px whitespace-nowrap">Rol</th>
                    <th class="w-full">Privilegios</th>
                  </tr>
                </thead>
                <tbody>
                  {#each columnGroups as group (pairKey(group.column, group.grantee))}
                    <tr>
                      <td class="w-px font-mono whitespace-nowrap">{group.column}</td>
                      <td class="w-px font-medium whitespace-nowrap">{group.grantee}</td>
                      <td>
                        <span class="flex flex-wrap gap-1">
                          {#each group.privileges as privilege, position (position)}
                            <span class="tag tag-neutral font-mono">{privilege}</span>
                          {/each}
                        </span>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}

          {#if isSchema && defaultGrants.length > 0}
            <div class="card mt-3 overflow-hidden">
              <div class="card-head">
                <span class="card-title">Por omisión</span>
                <span class="text-xs muted">lo que van a recibir los objetos que se creen acá</span>
              </div>
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Cuando crea</th>
                    <th class="w-px whitespace-nowrap">Sobre</th>
                    <th class="w-px whitespace-nowrap">Rol</th>
                    <th class="w-full">Privilegio</th>
                  </tr>
                </thead>
                <tbody>
                  {#each defaultGrants as grant, position (position)}
                    <tr>
                      <td class="w-px whitespace-nowrap">{grant.owner}</td>
                      <td class="w-px whitespace-nowrap">{grant.objects}</td>
                      <td class="w-px font-medium whitespace-nowrap">{grant.grantee}</td>
                      <td>
                        <span class="tag tag-neutral font-mono">{grant.privilege}</span>
                        {#if grant.grantable}
                          <span class="tag tag-info">con GRANT OPTION</span>
                        {/if}
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        {:else if section === "mappings"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">Mapeos de usuario</span>
              <button
                class="btn btn-sm ml-auto"
                {...blocked}
                onclick={() => (userMappingDialog = { existing: null })}
              >
                <Icon name="plus" size={11} />
                Mapeo
              </button>
            </div>

            {#if mappingsError}
              <Alert tone="bad" box class="m-3">{mappingsError}</Alert>
            {:else if mappings.length === 0}
              {@render nothing("No tiene mapeos de usuario.")}
            {:else}
              <table class="list-table">
                <thead>
                  <tr>
                    <th class="w-px whitespace-nowrap">Rol</th>
                    <th class="w-full">Opciones</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each mappings as mapping (mapping.user)}
                    <tr class="group">
                      <td class="w-px font-medium whitespace-nowrap">{mapping.user}</td>
                      <td class="max-w-0 truncate font-mono text-xs muted">
                        {#if mapping.options === null}
                          (ocultas)
                        {:else}
                          {mapping.options
                            .map(([key, value]) => (key === "password" ? `${key}=••••` : `${key}=${value}`))
                            .join(", ") || "—"}
                        {/if}
                      </td>
                      <td class="w-24">
                        <div class="row-actions">
                          <button
                            class="btn btn-ghost btn-icon size-6"
                            title="Editar el mapeo"
                            aria-label="Editar el mapeo"
                            {...blocked}
                            onclick={() => (userMappingDialog = { existing: mapping })}
                          >
                            <Icon name="edit" size={12} />
                          </button>
                          <button
                            class="btn btn-danger-ghost btn-icon size-6"
                            title="Quitar el mapeo"
                            aria-label="Quitar el mapeo"
                            onclick={() => (mappingDrop = { user: mapping.user })}
                          >
                            <Icon name="trash" size={12} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        {:else if section === "ddl"}
          <div class="card overflow-hidden">
            <div class="card-head">
              <span class="card-title">DDL</span>
              {#if ddl}
                <span class="text-xs muted">
                  {ddl.source === "pgDump" ? "reconstruido con pg_dump" : "generado por PostgreSQL"}
                </span>
                <span class="ml-auto flex items-center gap-1">
                  <FontSize />
                  <button class="btn btn-sm" onclick={copy}>
                    <Icon name={copied ? "check" : "copy"} size={11} />
                    {copied ? "Copiado" : "Copiar"}
                  </button>
                </span>
              {/if}
            </div>

            {#if loading}
              {@render pending("Generando DDL…")}
            {:else if ddlError}
              <Alert tone="bad" box class="m-3">{ddlError}</Alert>
            {:else if ddl}
              <div class="overflow-auto px-4 py-3 leading-relaxed">
                <Sql code={ddl.sql} />
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>

{#snippet pending(text: string)}
  <p class="flex items-center gap-2 px-3 py-4 text-sm muted">
    <span class="spinner"></span>
    {text}
  </p>
{/snippet}

{#snippet nothing(text: string)}
  <p class="px-3 py-4 text-sm muted">{text}</p>
{/snippet}

{#if newTable && selected && node?.schema}
  <TableDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    onclose={() => (newTable = false)}
    oncreated={afterTableCreated}
  />
{/if}

{#if columnDialog && selected && shape}
  <ColumnDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    column={columnDialog.column}
    onclose={() => (columnDialog = null)}
    onsaved={afterColumnSaved}
  />
{/if}

{#if newIndex && selected && shape}
  <IndexDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    columns={shape.columns}
    onclose={() => (newIndex = false)}
    oncreated={afterIndexCreated}
  />
{/if}

{#if newConstraint && selected && shape}
  <ConstraintDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    columns={shape.columns}
    onclose={() => (newConstraint = false)}
    oncreated={afterConstraintCreated}
  />
{/if}

{#if triggerDialog && selected && shape}
  <TriggerDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    existing={triggerDialog.existing}
    onclose={() => (triggerDialog = null)}
    onsaved={afterTriggerSaved}
  />
{/if}

{#if policyDialog && selected && shape}
  <PolicyDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    schema={shape.schema}
    table={shape.name}
    existing={policyDialog.existing}
    onclose={() => (policyDialog = null)}
    onsaved={afterPolicySaved}
  />
{/if}

{#if viewDialog && selected && node?.schema}
  <ViewDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    materialized={viewDialog.materialized}
    existing={viewDialog.existing}
    onclose={() => (viewDialog = null)}
    onsaved={afterViewSaved}
  />
{/if}

{#if sequenceDialog && selected && node?.schema}
  <SequenceDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    existing={sequenceDialog.existing}
    onclose={() => (sequenceDialog = null)}
    onsaved={() => {
      const wasCreate = sequenceDialog?.existing === null;
      sequenceDialog = null;
      if (wasCreate) {
        if (selected) explorer.reload(selected);
      } else {
        refreshDdl();
      }
    }}
  />
{/if}

{#if typeDialog && selected && node?.schema}
  <TypeDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    composite={typeDialog.composite}
    existing={typeDialog.existing}
    onclose={() => (typeDialog = null)}
    onsaved={() => {
      const wasCreate = typeDialog?.existing === null;
      typeDialog = null;
      if (wasCreate) {
        if (selected) explorer.reload(selected);
      } else {
        refreshDdl();
      }
    }}
  />
{/if}

{#if domainDialog && selected && node?.schema}
  <DomainDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    existing={domainDialog.existing}
    onclose={() => (domainDialog = null)}
    onsaved={() => {
      const wasCreate = domainDialog?.existing === null;
      domainDialog = null;
      if (wasCreate) {
        if (selected) explorer.reload(selected);
      } else {
        refreshDdl();
      }
    }}
  />
{/if}

{#if schemaDialog && selected && node}
  <SchemaDialog
    profileId={selected.profileId}
    database={node.database}
    existing={schemaDialog.existing}
    onclose={() => (schemaDialog = null)}
    onsaved={() => {
      schemaDialog = null;
      // Un esquema nuevo o renombrado cambia la lista de arriba, no el nodo de abajo.
      const parent = selected ? parentOf(explorer.roots, selected) : null;
      if (parent) explorer.reload(parent);
      else if (selected) explorer.reload(selected);
    }}
  />
{/if}

{#if databaseDialog && selected}
  <DatabaseDialog
    profileId={selected.profileId}
    existing={databaseDialog.existing}
    onclose={() => (databaseDialog = null)}
    onsaved={() => {
      databaseDialog = null;
      const parent = selected ? parentOf(explorer.roots, selected) : null;
      if (parent) explorer.reload(parent);
    }}
  />
{/if}

{#if partitionDialog && selected && node?.schema}
  <PartitionDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    parent={node.label}
    strategy={partitionDialog.strategy}
    onclose={() => (partitionDialog = null)}
    onsaved={() => {
      partitionDialog = null;
      if (selected) explorer.reload(selected);
    }}
  />
{/if}

{#if commentDialog && selected && node}
  <CommentDialog
    profileId={selected.profileId}
    database={node.database}
    target={commentDialog.target}
    label={commentDialog.label}
    current={commentDialog.current}
    onclose={() => (commentDialog = null)}
    onsaved={() => {
      commentDialog = null;
      const parent = selected ? parentOf(explorer.roots, selected) : null;
      if (parent) explorer.reload(parent);
    }}
  />
{/if}

{#if functionDialog && selected}
  <FunctionDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    sql={functionDialog.sql}
    onclose={() => (functionDialog = null)}
    onsaved={afterFunctionSaved}
  />
{/if}

{#if roleDialog && selected}
  <RoleDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    existing={roleDialog.existing}
    onclose={() => (roleDialog = null)}
    onsaved={afterRoleSaved}
  />
{/if}

{#if extensionDialog && selected && node}
  <ExtensionDialog
    profileId={selected.profileId}
    database={node.database}
    existing={extensionDialog.existing}
    onclose={() => (extensionDialog = null)}
    onsaved={afterExtensionSaved}
  />
{/if}

{#if fdwDialog && selected && node}
  <FdwDialog
    profileId={selected.profileId}
    database={node.database}
    existing={fdwDialog.existing}
    onclose={() => (fdwDialog = null)}
    onsaved={afterFdwSaved}
  />
{/if}

{#if foreignServerDialog && selected && node}
  <ForeignServerDialog
    profileId={selected.profileId}
    database={node.database}
    existing={foreignServerDialog.existing}
    onclose={() => (foreignServerDialog = null)}
    onsaved={afterForeignServerSaved}
  />
{/if}

{#if userMappingDialog && selected && node && isForeignServer}
  <UserMappingDialog
    profileId={selected.profileId}
    database={node.database}
    server={node.label}
    existing={userMappingDialog.existing}
    onclose={() => (userMappingDialog = null)}
    onsaved={afterMappingSaved}
  />
{/if}

{#if mappingDrop}
  <Confirm
    title="Quitar el mapeo"
    message="¿Quitar el mapeo de usuario de {mappingDrop.user}?"
    confirmLabel="Quitar"
    onconfirm={confirmMappingDrop}
    onclose={() => (mappingDrop = null)}
  />
{/if}

{#if privilegeDialog && selected && node && privilegeSubject}
  <PrivilegeDialog
    profileId={selected.profileId}
    database={node.database}
    subject={privilegeSubject}
    columns={isTable ? (shape?.columns.map((column) => column.name) ?? []) : []}
    existing={privilegeDialog.existing}
    onclose={() => (privilegeDialog = null)}
    onsaved={afterPrivilegeSaved}
  />
{/if}

{#if backupDialog && selected && node}
  <BackupDialog
    profileId={selected.profileId}
    database={node.label}
    onclose={() => (backupDialog = false)}
  />
{/if}

{#if restoreDialog && selected && node}
  <RestoreDialog
    profileId={selected.profileId}
    database={node.label}
    onclose={() => (restoreDialog = false)}
  />
{/if}

{#if exportDialog && selected && node && node.schema && node.database}
  <ExportDialog
    profileId={selected.profileId}
    database={node.database}
    source={{ kind: "table", schema: node.schema, table: node.label, columns: [] }}
    onclose={() => (exportDialog = false)}
  />
{/if}

{#if importDialog && selected && node && node.schema && node.database}
  <ImportDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    table={node.label}
    onclose={() => (importDialog = false)}
  />
{/if}

{#if revokeTarget}
  <Confirm
    title="Revocar privilegios"
    message="¿Revocarle todos los privilegios a {revokeTarget.grantee} ({revokeTarget.privileges
      .join(', ')
      .toUpperCase()})?"
    confirmLabel="Revocar"
    busy={revoking}
    error={revokeError}
    onconfirm={confirmRevoke}
    onclose={() => {
      revokeTarget = null;
      revokeCascade = false;
      revokeError = null;
    }}
  >
    <label class="check">
      <input type="checkbox" bind:checked={revokeCascade} />
      CASCADE (también revoca lo que depende de esto)
    </label>
  </Confirm>
{/if}

{#if refreshTarget}
  <Confirm
    title="Refrescar la vista materializada"
    message="¿Volver a calcular los datos de {refreshTarget.name}? Puede tardar tanto como la consulta que la define."
    confirmLabel="Refrescar"
    danger={false}
    busy={refreshing}
    error={refreshError}
    onconfirm={confirmRefresh}
    onclose={() => {
      refreshTarget = null;
      refreshConcurrently = false;
      refreshError = null;
    }}
  >
    <label class="check">
      <input type="checkbox" bind:checked={refreshConcurrently} />
      CONCURRENTLY (no bloquea a los lectores; necesita un índice único)
    </label>
  </Confirm>
{/if}

{#if dropTarget}
  <Confirm
    title="Eliminar"
    message={dropQuestion}
    confirmLabel="Eliminar"
    busy={dropping}
    error={dropError}
    onconfirm={confirmDrop}
    onclose={closeDropDialog}
  >
    {#if dropTarget.kind === "index"}
      <label class="check">
        <input type="checkbox" bind:checked={dropConcurrently} />
        CONCURRENTLY (no bloquea la tabla; no se puede combinar con CASCADE)
      </label>
    {:else if dropTarget.kind === "role"}
      <label class="check">
        <input type="checkbox" bind:checked={reassignFirst} />
        Reasignar primero lo que el rol posee
      </label>
      {#if reassignFirst}
        <label class="mt-2 flex flex-col gap-1">
          <span class="label">Nuevo dueño</span>
          <input class="field" bind:value={reassignTo} placeholder="CURRENT_USER" />
        </label>
        <p class="mt-1.5 text-xs muted">
          Se ejecuta un REASSIGN OWNED y un DROP OWNED, que solo alcanzan a la base conectada
          ({node?.database}). Si el rol tiene objetos en otra base, hay que repetirlo desde ahí.
        </p>
      {/if}
      <!-- Una base no admite CASCADE; lo que se le puede pedir es que eche a las sesiones. -->
    {:else if dropTarget.kind === "database"}
      <label class="check">
        <input type="checkbox" bind:checked={dropCascade} />
        FORCE (echa a las sesiones conectadas en vez de fallar)
      </label>
      <!-- Una política no admite CASCADE: nada puede depender de ella. -->
    {:else if dropTarget.kind !== "policy"}
      <label class="check">
        <input type="checkbox" bind:checked={dropCascade} />
        CASCADE (también borra lo que depende de esto)
      </label>
    {/if}
  </Confirm>
{/if}
