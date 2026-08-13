/**
 * Las acciones del panel de detalle: qué botones muestra la cabecera y qué hace cada borrado.
 *
 * Están acá y no adentro de `DetailPanel` por dos razones. La lista de botones era medio millar de
 * líneas de `{#if}` en el marcado, donde el orden —crear, abrir, editar, y recién al final lo que
 * borra— no se veía; como tabla de datos se lee de un vistazo y se prueba con Vitest. Y el borrado
 * repetía treinta veces el mismo cierre: buscar el padre en el árbol, recargarlo y limpiar la
 * selección. Acá cada caso dice **qué** quedó desactualizado y el panel se encarga una sola vez.
 */

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
  | "openQuery"
  | "openErd"
  | "export"
  | "import"
  | "backup"
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
    actions.push({ kind: "newTable", icon: "plus", label: "Nueva tabla", title: "Nueva tabla", tone: "primary" });
  }
  if (flags.isViewsFolder && schema) {
    actions.push({ kind: "newView", icon: "plus", label: "Nueva vista", title: "Nueva vista", tone: "primary", guarded: true });
  }
  if (flags.isMatViewsFolder && schema) {
    actions.push({
      kind: "newMaterializedView",
      icon: "plus",
      label: "Nueva vista materializada",
      title: "Nueva vista materializada",
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isFunctionsFolder && schema) {
    actions.push({ kind: "newFunction", icon: "plus", label: "Nueva función", title: "Nueva función", tone: "primary", guarded: true });
  }
  if (flags.isProceduresFolder && schema) {
    actions.push({
      kind: "newProcedure",
      icon: "plus",
      label: "Nuevo procedimiento",
      title: "Nuevo procedimiento",
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isRolesFolder) {
    actions.push({ kind: "newRole", icon: "plus", label: "Nuevo rol", title: "Nuevo rol", tone: "primary", guarded: true });
  }
  if (flags.isExtensionsFolder) {
    actions.push({
      kind: "installExtension",
      icon: "plus",
      label: "Instalar extensión",
      title: "Instalar extensión",
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isFdwsFolder) {
    actions.push({ kind: "newFdw", icon: "plus", label: "Nuevo wrapper", title: "Nuevo wrapper", tone: "primary", guarded: true });
  }
  if (flags.isForeignServersFolder) {
    actions.push({
      kind: "newForeignServer",
      icon: "plus",
      label: "Nuevo servidor foráneo",
      title: "Nuevo servidor foráneo",
      tone: "primary",
      guarded: true,
    });
  }
  if (flags.isSequencesFolder && schema) {
    actions.push({ kind: "newSequence", icon: "plus", label: "Nueva secuencia", title: "Nueva secuencia", tone: "primary", guarded: true });
  }
  if (flags.isTypesFolder && schema) {
    actions.push(
      { kind: "newEnum", icon: "plus", label: "Nueva enumeración", title: "Nueva enumeración", tone: "primary", guarded: true },
      { kind: "newComposite", icon: "plus", label: "Nuevo compuesto", title: "Nuevo compuesto", guarded: true },
      { kind: "newDomain", icon: "plus", label: "Nuevo dominio", title: "Nuevo dominio", guarded: true },
    );
  }
  if (flags.isSchemasFolder) {
    actions.push({ kind: "newSchema", icon: "plus", label: "Nuevo esquema", title: "Nuevo esquema", tone: "primary", guarded: true });
  }
  if (flags.isPartitionedTable && node?.oid) {
    actions.push({
      kind: "newPartition",
      icon: "plus",
      label: "Nueva partición",
      title: "Crea o engancha una partición de esta tabla",
      guarded: true,
    });
  }

  // --- Abrir en otra pestaña ---

  if (context.dataTarget !== null && context.queryTarget) {
    actions.push({ kind: "openData", icon: "table", label: "Datos", title: `Abre los datos de ${label}`, tone: "primary" });
  }
  if (context.queryTarget) {
    actions.push({
      kind: "openQuery",
      icon: "sql",
      label: "Consulta",
      title: `Abre una consulta contra ${context.queryTarget.database}`,
    });
  }
  if (flags.isSchema && node) {
    actions.push({
      kind: "openErd",
      icon: "diagram",
      label: "Diagrama",
      title: `Dibuja las tablas de ${node.label} y sus claves foráneas`,
    });
  }

  // --- Mover datos ---

  if (flags.isTable && node) {
    actions.push(
      { kind: "export", icon: "download", label: "Exportar", title: `Exporta ${node.label} a un archivo con COPY` },
      { kind: "import", icon: "upload", label: "Importar", title: `Importa un archivo a ${node.label} con COPY`, guarded: true },
    );
  }
  if (flags.isDatabase && node) {
    actions.push(
      { kind: "backup", icon: "download", label: "Backup", title: `Hace un backup de ${node.label} con pg_dump` },
      {
        kind: "restore",
        icon: "upload",
        label: "Restaurar",
        title: `Restaura un backup sobre ${node.label} con pg_restore`,
        guarded: true,
      },
    );
  }

  // --- Editar ---

  if (flags.isView && node) {
    actions.push({ kind: "editView", icon: "edit", label: "Editar", title: "Editar", guarded: true });
  }
  if (flags.isMaterializedView && node) {
    actions.push(
      { kind: "editMaterializedView", icon: "edit", label: "Editar", title: "Editar", guarded: true },
      {
        kind: "refreshMaterializedView",
        icon: "refresh",
        label: "Refrescar",
        title: "Vuelve a calcular los datos guardados de la vista",
        guarded: true,
      },
    );
  }
  if (flags.isRoutine && context.hasDdl) {
    actions.push({ kind: "editFunction", icon: "edit", label: "Editar", title: "Editar", guarded: true });
  }
  if (flags.isRole) actions.push({ kind: "editRole", icon: "edit", label: "Editar", title: "Editar" });
  if (flags.isExtension) actions.push({ kind: "editExtension", icon: "edit", label: "Editar", title: "Editar" });
  if (flags.isFdw) actions.push({ kind: "editFdw", icon: "edit", label: "Editar", title: "Editar" });
  if (flags.isForeignServer) {
    actions.push({ kind: "editForeignServer", icon: "edit", label: "Editar", title: "Editar" });
  }
  if (flags.isSequence && node?.oid) {
    actions.push({ kind: "editSequence", icon: "edit", label: "Editar", title: "Editar", guarded: true });
  }
  if (flags.isType && node?.oid) {
    actions.push({ kind: "editType", icon: "edit", label: "Editar", title: "Editar", guarded: true });
  }
  if (flags.isSchema && node) {
    actions.push({ kind: "editSchema", icon: "edit", label: "Editar", title: "Editar", guarded: true });
  }
  if (flags.isDatabase && node) {
    actions.push(
      { kind: "editDatabase", icon: "edit", label: "Editar", title: "Editar", guarded: true },
      { kind: "newDatabase", icon: "plus", label: "Nueva base", title: "Nueva base", guarded: true },
    );
  }
  if (context.hasCommentTarget) {
    actions.push({
      kind: "comment",
      icon: "edit",
      label: "Comentario",
      title: "Documenta el objeto adentro de la propia base",
      guarded: true,
    });
  }
  if (context.isGroup) {
    actions.push({ kind: "renameGroup", icon: "edit", label: "Renombrar", title: "Renombrar", tone: "primary" });
  }
  if (context.isServer) {
    actions.push(
      context.connected
        ? { kind: "disconnect", icon: "unplug", label: "Desconectar", title: "Desconectar" }
        : { kind: "connect", icon: "plug", label: "Conectar", title: "Conectar", tone: "primary" },
      { kind: "editServer", icon: "edit", label: "Editar", title: "Editar" },
    );
  }

  // --- Borrar ---

  if (flags.isTable && context.hasShape) {
    actions.push({ kind: "dropTable", icon: "trash", label: "Eliminar", title: "Eliminar la tabla", tone: "danger", guarded: true });
  }
  if (flags.isView && node) {
    actions.push({ kind: "dropView", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isMaterializedView && node) {
    actions.push({ kind: "dropMaterializedView", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isRoutine && context.hasDdl) {
    actions.push({ kind: "dropFunction", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isSequence && node) {
    actions.push({ kind: "dropSequence", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isType && node) {
    actions.push({ kind: "dropType", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isSchema && node) {
    actions.push({
      kind: "dropSchema",
      icon: "trash",
      label: "Eliminar",
      title: "Sin CASCADE falla si el esquema tiene algo adentro",
      tone: "danger",
      guarded: true,
    });
  }
  if (flags.isDatabase && node) {
    actions.push({ kind: "dropDatabase", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isRole) {
    actions.push({ kind: "dropRole", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isExtension) {
    actions.push({ kind: "dropExtension", icon: "trash", label: "Quitar", title: "Quitar", tone: "danger", guarded: true });
  }
  if (flags.isFdw) {
    actions.push({ kind: "dropFdw", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (flags.isForeignServer) {
    actions.push({ kind: "dropForeignServer", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger", guarded: true });
  }
  if (context.isServer) {
    actions.push({ kind: "deleteServer", icon: "trash", label: "Eliminar", title: "Eliminar", tone: "danger" });
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
      return `¿Eliminar la tabla ${target.label}? Se pierden sus datos.`;
    case "column":
      return `¿Eliminar la columna ${target.label}? Se pierden sus valores en todas las filas.`;
    case "index":
      return `¿Eliminar el índice ${target.label}?`;
    case "constraint":
      return `¿Eliminar la restricción ${target.label}?`;
    case "trigger":
      return `¿Eliminar el trigger ${target.label}?`;
    case "view":
      return `¿Eliminar la vista ${target.label}?`;
    case "materializedView":
      return `¿Eliminar la vista materializada ${target.label}? Se pierden los datos guardados.`;
    case "function":
      return `¿Eliminar ${target.procedure ? "el procedimiento" : "la función"} ${target.name}(${target.args})?`;
    case "policy":
      return `¿Eliminar la política ${target.label}? Si es la única que dejaba ver filas, la tabla queda sin nada visible.`;
    case "role":
      return `¿Eliminar el rol ${target.label}?`;
    case "extension":
      return `¿Quitar la extensión ${target.label} de la base?`;
    case "foreignDataWrapper":
      return `¿Eliminar el wrapper foráneo ${target.label}?`;
    case "foreignServer":
      return `¿Eliminar el servidor foráneo ${target.label}? Se pierden sus mapeos de usuario.`;
    case "sequence":
      return `¿Eliminar la secuencia ${target.label}?`;
    case "type":
      return `¿Eliminar el tipo ${target.label}? Las columnas que lo usan lo necesitan.`;
    case "domain":
      return `¿Eliminar el dominio ${target.label}? Las columnas que lo usan lo necesitan.`;
    case "schema":
      return `¿Eliminar el esquema ${target.label}? Sin CASCADE falla si tiene algo adentro.`;
    case "database":
      return `¿Eliminar la base ${target.label}? Se pierde todo lo que tiene adentro.`;
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
