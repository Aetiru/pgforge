/**
 * Las acciones del panel de detalle: qué botones muestra la cabecera y qué hace cada borrado.
 *
 * Están acá y no adentro de `DetailPanel` por dos razones. La lista de botones era medio millar de
 * líneas de `{#if}` en el marcado, donde el orden —crear, abrir, editar, y recién al final lo que
 * borra— no se veía; como tabla de datos se lee de un vistazo y se prueba con Vitest. Y el borrado
 * repetía treinta veces el mismo cierre: buscar el padre en el árbol, recargarlo y limpiar la
 * selección. Acá cada caso dice **qué** quedó desactualizado y el panel se encarga una sola vez.
 */

import { t } from "./i18n.svelte";
import type { IconName } from "./Icon.svelte";
import {
  databaseApply,
  ddlApply,
  domainApply,
  extensionApply,
  fdwApply,
  foreignServerApply,
  functionDrop,
  indexDrop,
  policyApply,
  roleApply,
  schemaApply,
  sequenceApply,
  triggerApply,
  typeApply,
  viewApply,
  type BookmarkTarget,
  type RoleChange,
  type TableShape,
  type TreeNode,
} from "./ipc";
import type { NodeFlags } from "./detail-node";
import type { QueryTarget } from "./tree-actions";

// ---------------------------------------------------------------------------
// Botones de la cabecera
// ---------------------------------------------------------------------------

export type DetailActionKind =
  | "newTable"
  | "newView"
  | "newMaterializedView"
  | "newFunction"
  | "newProcedure"
  | "newRole"
  | "installExtension"
  | "newFdw"
  | "newForeignServer"
  | "newSequence"
  | "newEnum"
  | "newComposite"
  | "newDomain"
  | "newSchema"
  | "newPartition"
  | "openData"
  | "openDatabase"
  | "openQuery"
  | "openErd"
  | "openCompare"
  | "compareData"
  | "openPermissionMatrix"
  | "openRoleCapabilities"
  | "export"
  | "import"
  | "transfer"
  | "backup"
  | "maintenance"
  | "restore"
  | "editView"
  | "editMaterializedView"
  | "refreshMaterializedView"
  | "editFunction"
  | "editRole"
  | "editExtension"
  | "editFdw"
  | "editForeignServer"
  | "editSequence"
  | "editType"
  | "editSchema"
  | "editDatabase"
  | "newDatabase"
  | "comment"
  | "bookmark"
  | "renameGroup"
  | "connect"
  | "disconnect"
  | "editServer"
  | "dropTable"
  | "dropView"
  | "dropMaterializedView"
  | "dropFunction"
  | "dropSequence"
  | "dropType"
  | "dropSchema"
  | "dropDatabase"
  | "dropRole"
  | "dropExtension"
  | "dropFdw"
  | "dropForeignServer"
  | "deleteServer";

export interface DetailAction {
  kind: DetailActionKind;
  icon: IconName;
  /** Lo que lee un lector de pantalla. */
  label: string;
  /** Lo que se ve al pasar por encima; explica cuando el ícono solo no alcanza. */
  title: string;
  tone?: "primary" | "danger";
  /** Se apaga, con su motivo, cuando el perfil es de solo lectura. */
  guarded?: boolean;
}

/** Lo que la cabecera necesita saber para decidir qué botones tiene sentido ofrecer. */
export interface ActionContext {
  flags: NodeFlags;
  node: TreeNode | null;
  isServer: boolean;
  isGroup: boolean;
  connected: boolean;
  /** El nombre que se muestra arriba: el del objeto, el del servidor o el de la carpeta. */
  label: string;
  /** La forma de la tabla ya leída; sin ella no hay qué borrar ni sobre qué crear una columna. */
  hasShape: boolean;
  /** El DDL ya generado, que es de donde sale el cuerpo de una función para editarla. */
  hasDdl: boolean;
  dataTarget: number | null;
  queryTarget: QueryTarget | null;
  hasCommentTarget: boolean;
  /** El objetivo de marcador de este nodo, o `null` si no es de las que se pueden marcar. */
  bookmarkTarget: BookmarkTarget | null;
  /** Si `bookmarkTarget` ya está marcado, para que el botón diga «Quitar» en vez de «Agregar». */
  bookmarked: boolean;
}

/**
 * Los botones de la cabecera, en orden. Primero lo que crea, después lo que abre otra pestaña,
 * después lo que edita, y al final lo que borra: se llega a ello después de todo lo demás.
 */
export function headerActions(context: ActionContext): DetailAction[] {
  const { flags, node, label } = context;
  const schema = node?.schema;
  const actions: DetailAction[] = [];

  // --- Crear ---

  if (flags.isTablesFolder && schema) {
    actions.push({ kind: "newTable", icon: "plus", label: t("Nueva tabla"), title: t("Nueva tabla"), tone: "primary" });
  }
  if (flags.isViewsFolder && schema) {
    actions.push({ kind: "newView", icon: "plus", label: t("Nueva vista"), title: t("Nueva vista"), tone: "primary", guarded: true });
  }
  if (flags.isMatViewsFolder && schema) {
    actions.push({
      kind: "newMaterializedView",
      icon: "plus",
      label: t("Nueva vista materializada"),
      title: t("Nueva vista materializada"),
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isFunctionsFolder && schema) {
    actions.push({ kind: "newFunction", icon: "plus", label: t("Nueva función"), title: t("Nueva función"), tone: "primary", guarded: true });
  }
  if (flags.isProceduresFolder && schema) {
    actions.push({
      kind: "newProcedure",
      icon: "plus",
      label: t("Nuevo procedimiento"),
      title: t("Nuevo procedimiento"),
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isRolesFolder) {
    actions.push({ kind: "newRole", icon: "plus", label: t("Nuevo rol"), title: t("Nuevo rol"), tone: "primary", guarded: true });
  }
  if (flags.isExtensionsFolder) {
    actions.push({
      kind: "installExtension",
      icon: "plus",
      label: t("Instalar extensión"),
      title: t("Instalar extensión"),
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isFdwsFolder) {
    actions.push({ kind: "newFdw", icon: "plus", label: t("Nuevo wrapper"), title: t("Nuevo wrapper"), tone: "primary", guarded: true });
  }
  if (flags.isForeignServersFolder) {
    actions.push({
      kind: "newForeignServer",
      icon: "plus",
      label: t("Nuevo servidor foráneo"),
      title: t("Nuevo servidor foráneo"),
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isSequencesFolder && schema) {
    actions.push({ kind: "newSequence", icon: "plus", label: t("Nueva secuencia"), title: t("Nueva secuencia"), tone: "primary", guarded: true });
  }
  if (flags.isTypesFolder && schema) {
    actions.push(
      { kind: "newEnum", icon: "plus", label: t("Nueva enumeración"), title: t("Nueva enumeración"), tone: "primary", guarded: true },
      { kind: "newComposite", icon: "plus", label: t("Nuevo compuesto"), title: t("Nuevo compuesto"), guarded: true },
      { kind: "newDomain", icon: "plus", label: t("Nuevo dominio"), title: t("Nuevo dominio"), guarded: true },
    );
  }
  if (flags.isSchemasFolder) {
    actions.push({ kind: "newSchema", icon: "plus", label: t("Nuevo esquema"), title: t("Nuevo esquema"), tone: "primary", guarded: true });
  }
  if (flags.isPartitionedTable && node?.oid) {
    actions.push({
      kind: "newPartition",
      icon: "plus",
      label: t("Nueva partición"),
      title: t("Crea o engancha una partición de esta tabla"),
      guarded: true,
    });
  }

  // --- Abrir en otra pestaña ---

  if (context.dataTarget !== null && context.queryTarget) {
    actions.push({ kind: "openData", icon: "table", label: t("Datos"), title: t("Abre los datos de {label}", { label }), tone: "primary" });
  }
  if (context.queryTarget) {
    actions.push({
      kind: "openQuery",
      icon: "sql",
      label: t("Consulta"),
      title: t("Abre una consulta contra {database}", { database: context.queryTarget.database }),
    });
  }
  if (flags.isDatabase && node) {
    actions.push({
      kind: "openDatabase",
      icon: "database",
      label: t("Abrir"),
      title: t("Abre {label} en una pestaña: propiedades, esquemas, tablas y extensiones", { label: node.label }),
      tone: "primary",
    });
  }
  if (flags.isSchema && node) {
    actions.push({
      kind: "openErd",
      icon: "diagram",
      label: t("Diagrama"),
      title: t("Dibuja las tablas de {label} y sus claves foráneas", { label: node.label }),
    });
    actions.push({
      kind: "openCompare",
      icon: "compare",
      label: t("Comparar"),
      title: t("Compara {label} contra el esquema de otro servidor conectado", { label: node.label }),
    });
    actions.push({
      kind: "openPermissionMatrix",
      icon: "role",
      label: t("Matriz de permisos"),
      title: t("Quién puede hacer qué sobre los objetos de {label}", { label: node.label }),
    });
  }
  if (flags.isRole && node) {
    actions.push({
      kind: "openRoleCapabilities",
      icon: "role",
      label: t("Qué puede hacer"),
      title: t("Los permisos calculados de {label} sobre un esquema elegido", { label: node.label }),
    });
  }

  // --- Marcar ---

  if (context.bookmarkTarget) {
    actions.push({
      kind: "bookmark",
      icon: "star",
      label: context.bookmarked ? t("Quitar de marcadores") : t("Agregar a marcadores"),
      title: context.bookmarked
        ? t("Sacar de la sección fija de marcadores")
        : t("Tenerlo siempre a mano en la sección fija de marcadores"),
    });
  }

  // --- Mantenimiento ---

  // VACUUM, ANALYZE y REINDEX vivían solo adentro del monitoreo, y quien buscaba «mantenimiento de
  // esta tabla» no lo encontraba parado sobre la tabla. El diálogo es el mismo.
  if (
    node &&
    (flags.isTable || flags.isMaterializedView || flags.isDatabase || node.kind === "index")
  ) {
    actions.push({
      kind: "maintenance",
      icon: "refresh",
      label: t("Mantenimiento"),
      title: t("VACUUM, ANALYZE o REINDEX sobre {label}", { label: node.label }),
      guarded: true,
    });
  }

  // --- Mover datos ---

  if (flags.isTable && node) {
    actions.push(
      { kind: "export", icon: "download", label: t("Exportar"), title: t("Exporta {label} a un archivo con COPY", { label: node.label }) },
      { kind: "import", icon: "upload", label: t("Importar"), title: t("Importa un archivo a {label} con COPY", { label: node.label }), guarded: true },
      // Escribe en el destino, no acá: el aviso de producción y el solo lectura se miran contra el
      // servidor elegido en el diálogo, no contra el de origen.
      { kind: "transfer", icon: "upload", label: t("Copiar a otro servidor"), title: t("Copia {label} a una tabla de otro servidor conectado, sin archivo intermedio", { label: node.label }) },
    );
    if (schema) {
      actions.push({
        kind: "compareData",
        icon: "compare",
        label: t("Comparar datos"),
        title: t("Compara las filas de {label} contra las de otra tabla y arma el SQL para igualarlas", { label: node.label }),
      });
    }
  }
  if (flags.isDatabase && node) {
    actions.push(
      { kind: "backup", icon: "download", label: t("Backup"), title: t("Hace un backup de {label} con pg_dump", { label: node.label }) },
      {
        kind: "restore",
        icon: "upload",
        label: t("Restaurar"),
        title: t("Restaura un backup sobre {label} con pg_restore", { label: node.label }),
        guarded: true,
      },
    );
  }

  // --- Editar ---

  if (flags.isView && node) {
    actions.push({ kind: "editView", icon: "edit", label: t("Editar"), title: t("Editar"), guarded: true });
  }
  if (flags.isMaterializedView && node) {
    actions.push(
      { kind: "editMaterializedView", icon: "edit", label: t("Editar"), title: t("Editar"), guarded: true },
      {
        kind: "refreshMaterializedView",
        icon: "refresh",
        label: t("Refrescar"),
        title: t("Vuelve a calcular los datos guardados de la vista"),
        guarded: true,
      },
    );
  }
  if (flags.isRoutine && context.hasDdl) {
    actions.push({ kind: "editFunction", icon: "edit", label: t("Editar"), title: t("Editar"), guarded: true });
  }
  if (flags.isRole) actions.push({ kind: "editRole", icon: "edit", label: t("Editar"), title: t("Editar") });
  if (flags.isExtension) actions.push({ kind: "editExtension", icon: "edit", label: t("Editar"), title: t("Editar") });
  if (flags.isFdw) actions.push({ kind: "editFdw", icon: "edit", label: t("Editar"), title: t("Editar") });
  if (flags.isForeignServer) {
    actions.push({ kind: "editForeignServer", icon: "edit", label: t("Editar"), title: t("Editar") });
  }
  if (flags.isSequence && node?.oid) {
    actions.push({ kind: "editSequence", icon: "edit", label: t("Editar"), title: t("Editar"), guarded: true });
  }
  if (flags.isType && node?.oid) {
    actions.push({ kind: "editType", icon: "edit", label: t("Editar"), title: t("Editar"), guarded: true });
  }
  if (flags.isSchema && node) {
    actions.push({ kind: "editSchema", icon: "edit", label: t("Editar"), title: t("Editar"), guarded: true });
  }
  if (flags.isDatabase && node) {
    actions.push(
      { kind: "editDatabase", icon: "edit", label: t("Editar"), title: t("Editar"), guarded: true },
      { kind: "newDatabase", icon: "plus", label: t("Nueva base"), title: t("Nueva base"), guarded: true },
    );
  }
  if (context.hasCommentTarget) {
    actions.push({
      kind: "comment",
      icon: "edit",
      label: t("Comentario"),
      title: t("Documenta el objeto adentro de la propia base"),
      guarded: true,
    });
  }
  if (context.isGroup) {
    actions.push({ kind: "renameGroup", icon: "edit", label: t("Renombrar"), title: t("Renombrar"), tone: "primary" });
  }
  if (context.isServer) {
    actions.push(
      context.connected
        ? { kind: "disconnect", icon: "unplug", label: t("Desconectar"), title: t("Desconectar") }
        : { kind: "connect", icon: "plug", label: t("Conectar"), title: t("Conectar"), tone: "primary" },
      { kind: "editServer", icon: "edit", label: t("Editar"), title: t("Editar") },
    );
  }

  // --- Borrar ---

  if (flags.isTable && context.hasShape) {
    actions.push({ kind: "dropTable", icon: "trash", label: t("Eliminar"), title: t("Eliminar la tabla"), tone: "danger", guarded: true });
  }
  if (flags.isView && node) {
    actions.push({ kind: "dropView", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isMaterializedView && node) {
    actions.push({ kind: "dropMaterializedView", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isRoutine && context.hasDdl) {
    actions.push({ kind: "dropFunction", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isSequence && node) {
    actions.push({ kind: "dropSequence", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isType && node) {
    actions.push({ kind: "dropType", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isSchema && node) {
    actions.push({
      kind: "dropSchema",
      icon: "trash",
      label: t("Eliminar"),
      title: t("Sin CASCADE falla si el esquema tiene algo adentro"),
      tone: "danger",
      guarded: true,
    });
  }
  if (flags.isDatabase && node) {
    actions.push({ kind: "dropDatabase", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isRole) {
    actions.push({ kind: "dropRole", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isExtension) {
    actions.push({ kind: "dropExtension", icon: "trash", label: t("Quitar"), title: t("Quitar"), tone: "danger", guarded: true });
  }
  if (flags.isFdw) {
    actions.push({ kind: "dropFdw", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (flags.isForeignServer) {
    actions.push({ kind: "dropForeignServer", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger", guarded: true });
  }
  if (context.isServer) {
    actions.push({ kind: "deleteServer", icon: "trash", label: t("Eliminar"), title: t("Eliminar"), tone: "danger" });
  }

  return actions;
}

// ---------------------------------------------------------------------------
// Borrado
// ---------------------------------------------------------------------------

/** Lo que se puede borrar desde el panel. La función lleva su firma: sin ella no hay `DROP`. */
export type DropTarget =
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
  | { kind: "function"; schema: string; name: string; args: string; procedure: boolean };

/**
 * Qué quedó desactualizado después de borrar.
 *
 * `parent` es el caso del objeto que desaparece del árbol; el resto son secciones del propio panel,
 * que sigue mirando la misma tabla.
 */
export type DropRefresh =
  | "shape"
  | "indexes"
  | "constraints"
  | "triggers"
  | "security"
  | "parent";

export interface DropOptions {
  cascade: boolean;
  /** Solo para índices: CASCADE y CONCURRENTLY son mutuamente excluyentes en Postgres. */
  concurrently: boolean;
  /**
   * Solo para roles: a quién pasarle lo que el rol posee antes de borrarlo, o `null` para no
   * reasignar. Un rol dueño de algo no se puede borrar y Postgres no tiene `DROP ROLE ... CASCADE`.
   */
  reassignTo: string | null;
}

export function dropQuestion(target: DropTarget): string {
  switch (target.kind) {
    case "table":
      return t("¿Eliminar la tabla {label}? Se pierden sus datos.", { label: target.label });
    case "column":
      return t("¿Eliminar la columna {label}? Se pierden sus valores en todas las filas.", { label: target.label });
    case "index":
      return t("¿Eliminar el índice {label}?", { label: target.label });
    case "constraint":
      return t("¿Eliminar la restricción {label}?", { label: target.label });
    case "trigger":
      return t("¿Eliminar el trigger {label}?", { label: target.label });
    case "view":
      return t("¿Eliminar la vista {label}?", { label: target.label });
    case "materializedView":
      return t("¿Eliminar la vista materializada {label}? Se pierden los datos guardados.", { label: target.label });
    case "function":
      return target.procedure
        ? t("¿Eliminar el procedimiento {signature}?", { signature: `${target.name}(${target.args})` })
        : t("¿Eliminar la función {signature}?", { signature: `${target.name}(${target.args})` });
    case "policy":
      return t("¿Eliminar la política {label}? Si es la única que dejaba ver filas, la tabla queda sin nada visible.", { label: target.label });
    case "role":
      return t("¿Eliminar el rol {label}?", { label: target.label });
    case "extension":
      return t("¿Quitar la extensión {label} de la base?", { label: target.label });
    case "foreignDataWrapper":
      return t("¿Eliminar el wrapper foráneo {label}?", { label: target.label });
    case "foreignServer":
      return t("¿Eliminar el servidor foráneo {label}? Se pierden sus mapeos de usuario.", { label: target.label });
    case "sequence":
      return t("¿Eliminar la secuencia {label}?", { label: target.label });
    case "type":
      return t("¿Eliminar el tipo {label}? Las columnas que lo usan lo necesitan.", { label: target.label });
    case "domain":
      return t("¿Eliminar el dominio {label}? Las columnas que lo usan lo necesitan.", { label: target.label });
    case "schema":
      return t("¿Eliminar el esquema {label}? Sin CASCADE falla si tiene algo adentro.", { label: target.label });
    case "database":
      return t("¿Eliminar la base {label}? Se pierde todo lo que tiene adentro.", { label: target.label });
  }
}

/**
 * Ejecuta el borrado y devuelve qué hay que releer después.
 *
 * Lo que cuelga de una tabla —columna, índice, restricción, trigger, política— se nombra con la
 * forma ya leída (`shape`) y no con el nodo: el nodo elegido es la tabla, y esos objetos viven en
 * sus secciones.
 */
export async function runDrop(
  profileId: string,
  target: DropTarget,
  node: TreeNode,
  shape: TableShape | null,
  options: DropOptions,
): Promise<DropRefresh[]> {
  const { cascade, concurrently, reassignTo } = options;
  const database = node.database;

  switch (target.kind) {
    case "table":
      await ddlApply(
        profileId,
        [{ kind: "dropTable", schema: shape!.schema, name: shape!.name, cascade }],
        database,
      );
      return ["parent"];
    case "column":
      await ddlApply(
        profileId,
        [
          {
            kind: "dropColumn",
            schema: shape!.schema,
            table: shape!.name,
            column: target.label,
            cascade,
          },
        ],
        database,
      );
      return ["shape"];
    case "index":
      await indexDrop(profileId, shape!.schema, target.label, cascade, concurrently, database);
      return ["indexes"];
    case "constraint":
      await ddlApply(
        profileId,
        [
          {
            kind: "dropConstraint",
            schema: shape!.schema,
            table: shape!.name,
            name: target.label,
            cascade,
          },
        ],
        database,
      );
      // Sacar una primary key o un unique puede volver de solo lectura a la grilla de datos.
      return ["constraints", "shape"];
    case "trigger":
      await triggerApply(
        profileId,
        [
          {
            kind: "dropTrigger",
            schema: shape!.schema,
            table: shape!.name,
            name: target.label,
            cascade,
          },
        ],
        database,
      );
      return ["triggers"];
    case "view":
      await viewApply(
        profileId,
        [{ kind: "dropView", schema: node.schema!, name: target.label, cascade }],
        database,
      );
      return ["parent"];
    case "materializedView":
      await viewApply(
        profileId,
        [{ kind: "dropMaterializedView", schema: node.schema!, name: target.label, cascade }],
        database,
      );
      return ["parent"];
    case "function":
      await functionDrop(
        profileId,
        target.schema,
        target.name,
        target.args,
        target.procedure,
        cascade,
        database,
      );
      return ["parent"];
    case "policy":
      await policyApply(
        profileId,
        [{ kind: "dropPolicy", schema: shape!.schema, table: shape!.name, name: target.label }],
        database,
      );
      return ["security"];
    case "role":
      await roleApply(
        profileId,
        [
          ...(reassignTo
            ? ([
                { kind: "reassignOwned", from: target.label, to: reassignTo },
                { kind: "dropOwned", role: target.label, cascade: false },
              ] as RoleChange[])
            : []),
          { kind: "dropRole", name: target.label },
        ],
        database,
      );
      return ["parent"];
    case "extension":
      await extensionApply(profileId, [{ kind: "drop", name: target.label, cascade }], database);
      return ["parent"];
    case "sequence":
      await sequenceApply(
        profileId,
        [{ kind: "dropSequence", schema: node.schema!, name: target.label, cascade }],
        database,
      );
      return ["parent"];
    case "type":
      await typeApply(
        profileId,
        [{ kind: "dropType", schema: node.schema!, name: target.label, cascade }],
        database,
      );
      return ["parent"];
    case "domain":
      await domainApply(
        profileId,
        [{ kind: "dropDomain", schema: node.schema!, name: target.label, cascade }],
        database,
      );
      return ["parent"];
    case "schema":
      await schemaApply(
        profileId,
        [{ kind: "dropSchema", name: target.label, ifExists: false, cascade }],
        database,
      );
      return ["parent"];
    case "database":
      // `force` echa a las sesiones conectadas: sin eso, una base con alguien adentro no se borra y
      // el error no dice quién es. Sin base: `DROP DATABASE` no corre contra la que se está borrando.
      await databaseApply(profileId, [
        { kind: "dropDatabase", name: target.label, ifExists: false, force: cascade },
      ]);
      return ["parent"];
    case "foreignDataWrapper":
      await fdwApply(profileId, [{ kind: "drop", name: target.label, cascade }], database);
      return ["parent"];
    case "foreignServer":
      await foreignServerApply(profileId, [{ kind: "drop", name: target.label, cascade }], database);
      return ["parent"];
  }
}
