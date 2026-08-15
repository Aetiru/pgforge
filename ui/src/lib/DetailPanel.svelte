<script lang="ts">
  /**
   * El panel del nodo elegido en el árbol.
   *
   * Lo que hace acá es coordinar: leer lo que cada sección necesita, abrir el diálogo que
   * corresponde y volver a leer lo que un cambio dejó viejo. Lo demás vive afuera —las banderas y
   * el vocabulario de cada objeto en `detail-node.ts`, los botones de la cabecera y el borrado en
   * `detail-actions.ts`, y cada sección en su componente de `detail/`—, que es lo que permite
   * probar con Vitest la parte que decide sin montar el panel entero.
   */
  import { untrack } from "svelte";
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
  import PrivilegeDialog, { type Existing as PrivilegeExisting } from "./PrivilegeDialog.svelte";
  import RoleDialog from "./RoleDialog.svelte";
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
  import Actions from "./detail/Actions.svelte";
  import Columns from "./detail/Columns.svelte";
  import Constraints from "./detail/Constraints.svelte";
  import DdlSection from "./detail/Ddl.svelte";
  import GroupServers from "./detail/GroupServers.svelte";
  import Indexes from "./detail/Indexes.svelte";
  import Mappings from "./detail/Mappings.svelte";
  import Privileges from "./detail/Privileges.svelte";
  import Properties from "./detail/Properties.svelte";
  import Security from "./detail/Security.svelte";
  import Triggers from "./detail/Triggers.svelte";
  import { confirmMutation, isReadOnly, readOnlyReason } from "./access.svelte";
  import { GROUP_LOOK, kindLabel, lookOf } from "./badges";
  import {
    headerActions,
    dropQuestion,
    runDrop,
    type DetailAction,
    type DropTarget,
  } from "./detail-actions";
  import {
    columnGroupsOf,
    commentTargetOf,
    flagsOf,
    functionSkeleton,
    pathOf,
    privilegeGroupsOf,
    privilegeSubjectOf,
    propertiesOf,
    type PrivilegeGroup,
  } from "./detail-node";
  import { explorer, type Row } from "./explorer.svelte";
  import { dataTargetOf, queryTargetOf } from "./tree-actions";
  import {
    dataOpen,
    databasePrivileges,
    columnPrivileges,
    defaultPrivileges,
    describeError,
    extensionInfo,
    fdwInfo,
    foreignServerInfo,
    functionArgs,
    functionPrivileges,
    isCanceled,
    objectDdl,
    policyApply,
    privilegeApply,
    readCancel,
    relationPrivileges,
    roleInfo,
    schemaPrivileges,
    tableConstraints,
    tableIndexes,
    tablePartitions,
    tableSecurity,
    tableTriggers,
    typeInfo,
    userMappingApply,
    userMappings,
    viewApply,
    type ColumnGrant,
    type CommentTarget,
    type CompareSide,
    type ConstraintInfo,
    type Ddl,
    type DefaultGrant,
    type ExtensionInfo,
    type FdwInfo,
    type Grantable,
    type IndexInfo,
    type PolicyChange,
    type PolicyInfo,
    type PrivilegeGrant,
    type RoleInfo,
    type ServerInfo,
    type TableColumn,
    type TableSecurity,
    type TableShape,
    type TriggerInfo,
    type UserMapping,
  } from "./ipc";

  let {
    onedit,
    ondelete,
    onconnect,
    ongroup,
    onquery,
    ondata,
    onerd,
    oncompare,
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
    /** Pide comparar este esquema contra otro; el otro lado lo elige un diálogo de `App`. */
    oncompare: (source: CompareSide) => void;
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
  const flags = $derived(flagsOf(node));

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

  $effect(() => {
    const current = node;
    ddl = null;
    ddlError = null;
    copied = false;

    if (!current || !flags.hasDdl || !selected) return;

    const profileId = selected.profileId;
    const request = crypto.randomUUID();
    let cancelled = false;
    let done = false;
    loading = true;

    objectDdl(profileId, current, request)
      .then((result) => {
        if (!cancelled) ddl = result;
      })
      .catch((error) => {
        if (!cancelled && !isCanceled(error)) ddlError = describeError(error);
      })
      .finally(() => {
        done = true;
        if (!cancelled) loading = false;
      });

    // Cambiar de nodo rápido no debe dejar que una respuesta vieja pise a la nueva; y si la lectura
    // todavía está corriendo, además se aborta contra el servidor. El DDL de una tabla lo genera
    // `pg_dump`: seguir esperándolo para tirarlo a la basura es trabajo del servidor y del disco.
    return () => {
      cancelled = true;
      if (!done) void readCancel(request).catch(() => {});
    };
  });

  /**
   * Re-lee el DDL del nodo actual sin esperar a que cambie de nodo. Hace falta después de editar
   * una vista: el nodo sigue siendo el mismo, así que el efecto de arriba no se vuelve a disparar
   * solo.
   */
  async function refreshDdl() {
    if (!node || !flags.hasDdl || !selected) return;
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

  // Las mismas reglas que usan el menú del clic derecho y Ctrl+Q: viven en `tree-actions.ts` para
  // que las tres puertas abran exactamente lo mismo. Las vistas y las materializadas también tienen
  // datos que mostrar: se abren en solo lectura, y el propio panel explica por qué.
  const queryTarget = $derived(queryTargetOf(selected, profile));
  const dataTarget = $derived(dataTargetOf(node));
  const commentTarget = $derived(commentTargetOf(node));

  // -------------------------------------------------------------------------
  // Estructura: lo que cada sección le pide al servidor
  // -------------------------------------------------------------------------

  let shape = $state<TableShape | null>(null);
  let shapeError = $state<string | null>(null);
  let shapeLoading = $state(false);

  async function loadShape() {
    if (!flags.isTable || !node?.oid || !selected) {
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
    if (!flags.isTable || !node?.oid || !selected) {
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
    if (!flags.isTable || !node?.oid || !selected) {
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
    if (!flags.isTable || !node?.oid || !selected) {
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
    if (!flags.isTable || !node?.oid || !selected) {
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
    if (!selected || !node || !flags.hasPrivileges) {
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

      if (flags.isDatabase) {
        // Sin pasar la base: `pg_database` es un catálogo compartido, y abrir un pool contra la
        // base que se está mirando fallaría justamente cuando no se tiene CONNECT sobre ella.
        privileges = await databasePrivileges(profileId, node.label);
      } else if (oid === undefined) {
        privileges = null;
      } else if (flags.isSchema) {
        privileges = await schemaPrivileges(profileId, oid, database);
        // Los privilegios por omisión no son de ningún objeto: se guardan por esquema, así que es
        // acá donde alguien los va a buscar.
        defaultGrants = (await defaultPrivileges(profileId, database)).filter(
          (grant) => grant.schema === node.label,
        );
      } else if (flags.isRoutine) {
        privileges = await functionPrivileges(profileId, oid, database);
        routineArgs = await functionArgs(profileId, oid, database);
      } else {
        privileges = await relationPrivileges(profileId, oid, database);
        if (flags.isTable) columnGrants = await columnPrivileges(profileId, oid, database);
      }
    } catch (error) {
      privilegesError = describeError(error);
    } finally {
      privilegesLoading = false;
    }
  }

  const privilegeSubject = $derived(privilegeSubjectOf(node, flags, routineArgs));
  const privilegeGroups = $derived(privilegeGroupsOf(privileges));
  const columnGroups = $derived(columnGroupsOf(columnGrants));

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
    if (flags.isDatabase) {
      return [{ id: "info", label: "Propiedades", count: null }, privilegeSection];
    }
    if (flags.isTable) {
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
    if (flags.isForeignServer) {
      return [
        { id: "mappings", label: "Mapeos de usuario", count: mappings.length },
        { id: "ddl", label: "DDL", count: null },
      ];
    }
    if (flags.hasPrivileges && flags.hasDdl) {
      return [privilegeSection, { id: "ddl", label: "DDL", count: null }];
    }
    if (flags.hasDdl) return [{ id: "ddl", label: "DDL", count: null }];
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

  const properties = $derived(propertiesOf(isServer, node, profile, caps));
  const path = $derived(pathOf(isServer, node, profile));

  // -------------------------------------------------------------------------
  // El árbol después de un cambio
  // -------------------------------------------------------------------------

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

  /**
   * Recarga la carpeta que contiene al nodo elegido y suelta la selección.
   *
   * Es el cierre de todo lo que hace desaparecer o renombrar un objeto: la fila que estaba elegida
   * ya no coincide con lo que quedó en el servidor.
   */
  async function reloadParentAndClear() {
    if (!selected) return;
    const parent = parentOf(explorer.roots, selected);
    if (parent) await explorer.reload(parent);
    explorer.selected = null;
  }

  /** Un objeto nuevo cuelga del nodo elegido —una carpeta—, así que se recarga ese mismo nodo. */
  function reloadSelected() {
    if (selected) explorer.reload(selected);
  }

  /** El cierre de crear o editar: lo primero suma una fila, lo segundo cambia la que ya estaba. */
  function afterSaved(wasCreate: boolean) {
    if (wasCreate) reloadSelected();
    else void reloadParentAndClear();
  }

  // -------------------------------------------------------------------------
  // Diálogos
  // -------------------------------------------------------------------------

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
  let commentDialog = $state<{
    target: CommentTarget;
    label: string;
    current: string | null;
  } | null>(null);
  let exportDialog = $state(false);
  let importDialog = $state(false);
  let revokeTarget = $state<{ grantee: string; privileges: string[] } | null>(null);
  let revokeCascade = $state(false);
  let revoking = $state(false);
  let revokeError = $state<string | null>(null);
  let dropTarget = $state<DropTarget | null>(null);
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

  // -------------------------------------------------------------------------
  // La barra de la cabecera
  // -------------------------------------------------------------------------

  const actions = $derived(
    selected
      ? headerActions({
          flags,
          node,
          isServer,
          isGroup,
          connected: selected.connected,
          label: selected.label,
          hasShape: shape !== null,
          hasDdl: ddl !== null,
          dataTarget,
          queryTarget,
          hasCommentTarget: commentTarget !== null,
        })
      : [],
  );

  /**
   * Qué hace cada botón de la cabecera. Casi todos abren un diálogo; los pocos que preguntan algo
   * al servidor antes —la firma de una función, la estrategia de una partición— lo hacen porque sin
   * esa respuesta el formulario no se puede armar.
   */
  function run(action: DetailAction) {
    if (!selected) return;
    const current = node;

    switch (action.kind) {
      case "newTable":
        newTable = true;
        break;
      case "newView":
        viewDialog = { materialized: false, existing: null };
        break;
      case "newMaterializedView":
        viewDialog = { materialized: true, existing: null };
        break;
      case "newFunction":
        functionDialog = { sql: functionSkeleton(current!.schema!, false), isEdit: false };
        break;
      case "newProcedure":
        functionDialog = { sql: functionSkeleton(current!.schema!, true), isEdit: false };
        break;
      case "newRole":
        roleDialog = { existing: null };
        break;
      case "installExtension":
        extensionDialog = { existing: null };
        break;
      case "newFdw":
        fdwDialog = { existing: null };
        break;
      case "newForeignServer":
        foreignServerDialog = { existing: null };
        break;
      case "newSequence":
        sequenceDialog = { existing: null };
        break;
      case "newEnum":
        typeDialog = { composite: false, existing: null };
        break;
      case "newComposite":
        typeDialog = { composite: true, existing: null };
        break;
      case "newDomain":
        domainDialog = { existing: null };
        break;
      case "newSchema":
        schemaDialog = { existing: null };
        break;
      case "newPartition":
        void openPartitionDialog();
        break;
      case "openData":
        ondata(selected.profileId, queryTarget!.database, queryTarget!.title, dataTarget!);
        break;
      case "openQuery":
        onquery(selected.profileId, queryTarget!.database, queryTarget!.title);
        break;
      case "openErd":
        onerd(selected.profileId, current!.database, current!.label);
        break;
      case "openCompare":
        oncompare({
          id: selected.profileId,
          database: current!.database,
          schema: current!.label,
        });
        break;
      case "export":
        exportDialog = true;
        break;
      case "import":
        importDialog = true;
        break;
      case "backup":
        backupDialog = true;
        break;
      case "restore":
        restoreDialog = true;
        break;
      case "editView":
        viewDialog = {
          materialized: false,
          existing: { oid: current!.oid!, name: current!.label },
        };
        break;
      case "editMaterializedView":
        viewDialog = { materialized: true, existing: { oid: current!.oid!, name: current!.label } };
        break;
      case "refreshMaterializedView":
        refreshTarget = { schema: current!.schema!, name: current!.label };
        break;
      case "editFunction":
        functionDialog = { sql: ddl!.sql, isEdit: true };
        break;
      case "editRole":
        void openEditRole();
        break;
      case "editExtension":
        void openEditExtension();
        break;
      case "editFdw":
        void openEditFdw();
        break;
      case "editForeignServer":
        void openEditForeignServer();
        break;
      case "editSequence":
        sequenceDialog = { existing: { oid: current!.oid!, name: current!.label } };
        break;
      case "editType":
        void openEditType();
        break;
      case "editSchema":
        schemaDialog = { existing: { name: current!.label, owner: current!.detail ?? "" } };
        break;
      case "editDatabase":
        databaseDialog = { existing: current!.label };
        break;
      case "newDatabase":
        databaseDialog = { existing: null };
        break;
      case "comment":
        commentDialog = {
          target: commentTarget!,
          label: selected.label,
          current: current?.comment ?? null,
        };
        break;
      case "renameGroup":
        ongroup(selected.group!);
        break;
      case "connect":
        onconnect(selected.profileId);
        break;
      case "disconnect":
        explorer.disconnect(selected.profileId);
        break;
      case "editServer":
        onedit(selected.profileId);
        break;
      case "deleteServer":
        ondelete(selected.profileId);
        break;
      case "dropTable":
        dropTarget = { kind: "table", label: shape!.name };
        break;
      case "dropView":
        dropTarget = { kind: "view", label: current!.label };
        break;
      case "dropMaterializedView":
        dropTarget = { kind: "materializedView", label: current!.label };
        break;
      case "dropFunction":
        void askDropFunction();
        break;
      case "dropSequence":
        dropTarget = { kind: "sequence", label: current!.label };
        break;
      case "dropType":
        dropTarget = { kind: "type", label: current!.label };
        break;
      case "dropSchema":
        dropTarget = { kind: "schema", label: current!.label };
        break;
      case "dropDatabase":
        dropTarget = { kind: "database", label: current!.label };
        break;
      case "dropRole":
        dropTarget = { kind: "role", label: selected.label };
        break;
      case "dropExtension":
        dropTarget = { kind: "extension", label: selected.label };
        break;
      case "dropFdw":
        dropTarget = { kind: "foreignDataWrapper", label: selected.label };
        break;
      case "dropForeignServer":
        dropTarget = { kind: "foreignServer", label: selected.label };
        break;
    }
  }

  // -------------------------------------------------------------------------
  // Lo que hay que preguntarle al servidor antes de abrir un formulario
  // -------------------------------------------------------------------------

  /**
   * Trae el rol tal como ya existe antes de abrir la edición: acá no hay nada precargado, a
   * diferencia de una columna, que ya viene con `shape`.
   */
  async function openEditRole() {
    if (!selected || !node?.oid) return;
    try {
      roleDialog = { existing: await roleInfo(selected.profileId, node.oid, node.database) };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  /** Trae la extensión tal como está antes de abrir la edición. */
  async function openEditExtension() {
    if (!selected || !node) return;
    try {
      extensionDialog = {
        existing: await extensionInfo(selected.profileId, node.label, node.database),
      };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

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
        procedure: flags.isProcedure,
      };
    } catch (error) {
      ddlError = describeError(error);
    }
  }

  // -------------------------------------------------------------------------
  // Datos externos (wrappers, servidores foráneos, mapeos)
  // -------------------------------------------------------------------------

  /** Al abrir el detalle de un servidor foráneo se listan sus mapeos de usuario. */
  $effect(() => {
    if (!flags.isForeignServer || !selected || !node) {
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

  function reloadMappings() {
    if (!selected || !node) return;
    userMappings(selected.profileId, node.label, node.database)
      .then((result) => (mappings = result))
      .catch((error) => (mappingsError = describeError(error)));
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

  // -------------------------------------------------------------------------
  // Privilegios
  // -------------------------------------------------------------------------

  function editPrivileges(group: PrivilegeGroup) {
    privilegeDialog = {
      existing: {
        grantee: group.grantee,
        privileges: group.privileges,
        grantable: group.grantable,
        columns: columnGrants.filter((grant) => grant.grantee === group.grantee),
      },
    };
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

  // -------------------------------------------------------------------------
  // Refrescar y borrar
  // -------------------------------------------------------------------------

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

  function closeDropDialog() {
    dropTarget = null;
    dropCascade = false;
    dropConcurrently = false;
    reassignFirst = false;
    reassignTo = "CURRENT_USER";
    dropError = null;
  }

  async function confirmDrop() {
    if (!dropTarget || !selected || !node) return;
    if (!(await confirmMutation(selected.profileId, "Se va a eliminar un objeto del servidor.")))
      return;
    dropping = true;
    dropError = null;
    try {
      const stale = await runDrop(selected.profileId, dropTarget, node, shape, {
        cascade: dropCascade,
        concurrently: dropConcurrently,
        reassignTo: reassignFirst ? reassignTo.trim() || "CURRENT_USER" : null,
      });

      for (const what of stale) {
        if (what === "parent") await reloadParentAndClear();
        else if (what === "shape") await loadShape();
        else if (what === "indexes") await loadIndexes();
        else if (what === "constraints") await loadConstraints();
        else if (what === "triggers") await loadTriggers();
        else if (what === "security") await loadSecurity();
      }
      closeDropDialog();
    } catch (error) {
      dropError = describeError(error);
    } finally {
      dropping = false;
    }
  }
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

        <Actions {actions} {blocked} onaction={run} />
      </div>

      {#if selected.comment}
        <p class="mt-2 text-sm text-zinc-600 dark:text-zinc-300">{selected.comment}</p>
      {/if}
    </header>

    {#if isGroup}
      <div class="min-h-0 flex-1 overflow-auto p-4">
        <GroupServers
          servers={groupServers}
          {onconnect}
          ondisconnect={(profileId) => explorer.disconnect(profileId)}
          onedit={(profileId) => onedit(profileId)}
        />
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
    {:else if flags.isFolder && sections.length === 0}
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
          <Properties rows={properties} />
        {:else if section === "columns"}
          <Columns
            {shape}
            loading={shapeLoading}
            error={shapeError}
            {blocked}
            onnew={() => (columnDialog = { column: null })}
            onedit={(column) => (columnDialog = { column })}
            ondrop={(column) => (dropTarget = { kind: "column", label: column })}
          />
        {:else if section === "indexes"}
          <Indexes
            {indexes}
            loading={indexesLoading}
            error={indexesError}
            canCreate={shape !== null}
            {blocked}
            onnew={() => (newIndex = true)}
            ondrop={(name) => (dropTarget = { kind: "index", label: name })}
          />
        {:else if section === "constraints"}
          <Constraints
            {constraints}
            loading={constraintsLoading}
            error={constraintsError}
            canCreate={shape !== null}
            {blocked}
            onnew={() => (newConstraint = true)}
            ondrop={(name) => (dropTarget = { kind: "constraint", label: name })}
          />
        {:else if section === "triggers"}
          <Triggers
            {triggers}
            loading={triggersLoading}
            error={triggersError}
            canCreate={shape !== null}
            {blocked}
            onnew={() => (triggerDialog = { existing: null })}
            onedit={(trigger) => (triggerDialog = { existing: trigger })}
            ondrop={(name) => (dropTarget = { kind: "trigger", label: name })}
          />
        {:else if section === "security"}
          <Security
            {security}
            loading={securityLoading}
            error={securityError}
            canCreate={shape !== null}
            {blocked}
            onnew={() => (policyDialog = { existing: null })}
            onedit={(policy) => (policyDialog = { existing: policy })}
            ondrop={(name) => (dropTarget = { kind: "policy", label: name })}
            onenabled={(enabled) =>
              applySwitch({
                kind: "setRowSecurity",
                schema: shape!.schema,
                table: shape!.name,
                enabled,
              })}
            onforced={(forced) =>
              applySwitch({
                kind: "setForceRowSecurity",
                schema: shape!.schema,
                table: shape!.name,
                forced,
              })}
          />
        {:else if section === "privileges"}
          <Privileges
            groups={privilegeGroups}
            {columnGroups}
            defaultGrants={flags.isSchema ? defaultGrants : []}
            loading={privilegesLoading}
            error={privilegesError}
            {blocked}
            onnew={() => (privilegeDialog = { existing: null })}
            onedit={editPrivileges}
            onrevoke={(group) =>
              (revokeTarget = { grantee: group.grantee, privileges: group.privileges })}
          />
        {:else if section === "mappings"}
          <Mappings
            {mappings}
            error={mappingsError}
            {blocked}
            onnew={() => (userMappingDialog = { existing: null })}
            onedit={(mapping) => (userMappingDialog = { existing: mapping })}
            ondrop={(user) => (mappingDrop = { user })}
          />
        {:else if section === "ddl"}
          <DdlSection {ddl} {loading} error={ddlError} {copied} oncopy={copy} />
        {/if}
      </div>
    {/if}
  {/if}
</div>

{#if newTable && selected && node?.schema}
  <TableDialog
    profileId={selected.profileId}
    database={node.database}
    schema={node.schema}
    onclose={() => (newTable = false)}
    oncreated={() => {
      newTable = false;
      reloadSelected();
    }}
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
    onsaved={() => {
      columnDialog = null;
      loadShape();
    }}
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
    oncreated={() => {
      newIndex = false;
      loadIndexes();
    }}
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
    oncreated={() => {
      newConstraint = false;
      loadConstraints();
      // Agregar una primary key o un unique puede cambiar si la grilla de datos de la tabla es
      // editable: se relee la forma para que ese estado no quede desactualizado en el panel.
      loadShape();
    }}
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
    onsaved={() => {
      triggerDialog = null;
      loadTriggers();
    }}
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
    onsaved={() => {
      policyDialog = null;
      loadSecurity();
    }}
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
    onsaved={() => {
      const wasCreate = viewDialog?.existing === null;
      viewDialog = null;
      if (wasCreate) reloadSelected();
      else refreshDdl();
    }}
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
      if (wasCreate) reloadSelected();
      else refreshDdl();
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
      if (wasCreate) reloadSelected();
      else refreshDdl();
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
      if (wasCreate) reloadSelected();
      else refreshDdl();
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
      else reloadSelected();
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
      reloadSelected();
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
    onsaved={() => {
      const wasCreate = functionDialog !== null && !functionDialog.isEdit;
      functionDialog = null;
      if (wasCreate) reloadSelected();
      else refreshDdl();
    }}
  />
{/if}

{#if roleDialog && selected}
  <RoleDialog
    profileId={selected.profileId}
    database={node?.database ?? ""}
    existing={roleDialog.existing}
    onclose={() => (roleDialog = null)}
    onsaved={() => {
      const wasCreate = roleDialog?.existing == null;
      roleDialog = null;
      afterSaved(wasCreate);
    }}
  />
{/if}

{#if extensionDialog && selected && node}
  <ExtensionDialog
    profileId={selected.profileId}
    database={node.database}
    existing={extensionDialog.existing}
    onclose={() => (extensionDialog = null)}
    onsaved={() => {
      // Instalar suma un nodo a la carpeta; actualizar o cambiar de esquema cambia el detalle del
      // nodo, y entonces lo que hay que releer es la carpeta que lo contiene.
      const wasInstall = extensionDialog?.existing == null;
      extensionDialog = null;
      afterSaved(wasInstall);
    }}
  />
{/if}

{#if fdwDialog && selected && node}
  <FdwDialog
    profileId={selected.profileId}
    database={node.database}
    existing={fdwDialog.existing}
    onclose={() => (fdwDialog = null)}
    onsaved={() => {
      const wasCreate = fdwDialog?.existing == null;
      fdwDialog = null;
      afterSaved(wasCreate);
    }}
  />
{/if}

{#if foreignServerDialog && selected && node}
  <ForeignServerDialog
    profileId={selected.profileId}
    database={node.database}
    existing={foreignServerDialog.existing}
    onclose={() => (foreignServerDialog = null)}
    onsaved={() => {
      const wasCreate = foreignServerDialog?.existing == null;
      foreignServerDialog = null;
      afterSaved(wasCreate);
    }}
  />
{/if}

{#if userMappingDialog && selected && node && flags.isForeignServer}
  <UserMappingDialog
    profileId={selected.profileId}
    database={node.database}
    server={node.label}
    existing={userMappingDialog.existing}
    onclose={() => (userMappingDialog = null)}
    onsaved={() => {
      userMappingDialog = null;
      reloadMappings();
    }}
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
    columns={flags.isTable ? (shape?.columns.map((column) => column.name) ?? []) : []}
    existing={privilegeDialog.existing}
    onclose={() => (privilegeDialog = null)}
    onsaved={() => {
      privilegeDialog = null;
      loadPrivileges();
    }}
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
    message={dropQuestion(dropTarget)}
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
