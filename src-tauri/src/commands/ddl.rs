//! Estructura de tablas: crear, cambiar y borrar tablas, columnas, índices, constraints, vistas,
//! funciones, triggers, roles, privilegios y políticas de seguridad por fila.

use pgforge_core::ddl::comment::{self, CommentChange};
use pgforge_core::ddl::database::{self, DatabaseChange, DatabaseInfo};
use pgforge_core::ddl::domain::{self, DomainChange, DomainInfo};
use pgforge_core::ddl::extension::{self, AvailableExtension, ExtensionChange, ExtensionInfo};
use pgforge_core::ddl::fdw::{
    self, FdwChange, FdwInfo, ServerChange, ServerInfo, UserMapping, UserMappingChange,
};
use pgforge_core::ddl::function;
use pgforge_core::ddl::index::{self, IndexDef, IndexInfo};
use pgforge_core::ddl::partition::{self, PartitionChange, PartitioningInfo};
use pgforge_core::ddl::policy::{self, PolicyChange, TableSecurity};
use pgforge_core::ddl::privilege::{
    self, ColumnGrant, DefaultGrant, PrivilegeChange, PrivilegeGrant,
};
use pgforge_core::ddl::role::{self, RoleChange, RoleInfo};
use pgforge_core::ddl::schema::{self, SchemaChange};
use pgforge_core::ddl::sequence::{self, SequenceChange, SequenceInfo};
use pgforge_core::ddl::table::{self, ConstraintInfo, Statement, TableChange};
use pgforge_core::ddl::trigger::{self, TriggerChange, TriggerInfo};
use pgforge_core::ddl::types::{self, TypeChange, TypeInfo};
use pgforge_core::ddl::view::{self, ViewChange};
use pgforge_core::{ProfileId, Result};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::tasks::{self, TaskEvent};
use crate::commands::{record_applied, sql_of};
use crate::state::AppState;

/// El SQL que se ejecutaría, sin ejecutar nada. No toca la red ni el estado: la vista previa
/// muestra exactamente lo que se va a correr, igual que `data_preview`.
#[tauri::command]
pub fn ddl_preview(changes: Vec<TableChange>) -> Result<Vec<Statement>> {
    table::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn ddl_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<TableChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&table::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        table::apply(&handle, &database, &changes),
    )
    .await
}

/// Las constraints que ya tiene una tabla.
#[tauri::command]
pub async fn table_constraints(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<Vec<ConstraintInfo>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    table::constraints(&handle, &database, oid).await
}

/// El SQL de un índice nuevo, sin crearlo.
#[tauri::command]
pub fn index_preview(def: IndexDef) -> Result<Statement> {
    index::create_sql(&def)
}

/// Crea un índice y devuelve el identificador de la tarea, con el que se la puede cancelar.
///
/// Corre en segundo plano por la misma razón que el mantenimiento: un `CREATE INDEX CONCURRENTLY`
/// sobre una tabla grande tarda lo que tarda, y hasta que terminara la aplicación entera quedaba
/// esperando. Nunca en transacción, que es justamente lo que permite `CONCURRENTLY`.
#[tauri::command]
pub async fn index_create(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    def: IndexDef,
    channel: Channel<TaskEvent>,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    let statement = index::create_sql(&def)?;

    tasks::spawn_statement(app, &state, id, database, statement.sql, channel).await
}

/// Borra un índice.
#[tauri::command]
pub async fn index_drop(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    schema: String,
    name: String,
    cascade: bool,
    concurrently: bool,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    index::drop_index(&handle, &database, &schema, &name, cascade, concurrently).await
}

/// Los índices que ya tiene una tabla.
#[tauri::command]
pub async fn table_indexes(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<Vec<IndexInfo>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    index::indexes(&handle, &database, oid).await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn view_preview(changes: Vec<ViewChange>) -> Result<Vec<Statement>> {
    view::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn view_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<ViewChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&view::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        view::apply(&handle, &database, &changes),
    )
    .await
}

/// El cuerpo del `SELECT` de una vista, para precargar el editor al abrir "Editar".
#[tauri::command]
pub async fn view_query(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    view::query_of(&handle, &database, oid).await
}

/// Ejecuta la sentencia `CREATE [OR REPLACE] FUNCTION`/`CREATE [OR REPLACE] PROCEDURE` tal cual.
#[tauri::command]
pub async fn function_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    sql: String,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    function::apply(&handle, &database, &sql).await
}

/// Borra una función o un procedimiento.
///
/// Los argumentos de un comando de Tauri son la carga del `invoke`, no una firma que se pueda
/// reagrupar: juntarlos en un struct los movería también en `ipc.ts` sin sacar ninguno.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn function_drop(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    schema: String,
    name: String,
    args: String,
    procedure: bool,
    cascade: bool,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    function::drop(
        &handle, &database, &schema, &name, &args, procedure, cascade,
    )
    .await
}

/// La lista de tipos de argumento, para poder armar el `DROP FUNCTION`/`DROP PROCEDURE`.
#[tauri::command]
pub async fn function_args(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    function::identity_args(&handle, &database, oid).await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn trigger_preview(changes: Vec<TriggerChange>) -> Result<Vec<Statement>> {
    trigger::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn trigger_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<TriggerChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&trigger::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        trigger::apply(&handle, &database, &changes),
    )
    .await
}

/// Los triggers que ya tiene una tabla.
#[tauri::command]
pub async fn table_triggers(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<Vec<TriggerInfo>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    trigger::triggers(&handle, &database, oid).await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn role_preview(changes: Vec<RoleChange>) -> Result<Vec<Statement>> {
    role::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn role_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<RoleChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&role::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        role::apply(&handle, &database, &changes),
    )
    .await
}

/// El rol tal como ya existe, para precargar el diálogo de edición.
#[tauri::command]
pub async fn role_info(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<RoleInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    role::role(&handle, &database, oid).await
}

/// De qué roles es miembro, para precargar "miembro de" en el diálogo de edición.
#[tauri::command]
pub async fn role_memberships(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    name: String,
) -> Result<Vec<String>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    role::role_memberships(&handle, &database, &name).await
}

/// Los roles que existen en el servidor, para el selector de "miembro de".
#[tauri::command]
pub async fn role_names(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
) -> Result<Vec<String>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    role::role_names(&handle, &database).await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn privilege_preview(changes: Vec<PrivilegeChange>) -> Result<Vec<Statement>> {
    privilege::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn privilege_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<PrivilegeChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&privilege::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        privilege::apply(&handle, &database, &changes),
    )
    .await
}

/// Los privilegios de una tabla, una vista, una vista materializada o una secuencia.
#[tauri::command]
pub async fn relation_privileges(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<Vec<PrivilegeGrant>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    privilege::relation_privileges(&handle, &database, oid).await
}

/// Los privilegios de una función o un procedimiento.
#[tauri::command]
pub async fn function_privileges(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<Vec<PrivilegeGrant>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    privilege::function_privileges(&handle, &database, oid).await
}

/// Los privilegios de una base. Se busca por nombre y no por oid: es lo que tiene a mano el árbol.
#[tauri::command]
pub async fn database_privileges(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    name: String,
) -> Result<Vec<PrivilegeGrant>> {
    let handle = state.manager.require(id).await?;
    let connected = database.unwrap_or_else(|| handle.default_database().to_owned());

    privilege::database_privileges(&handle, &connected, &name).await
}

/// Los privilegios acotados a columnas de una tabla.
#[tauri::command]
pub async fn column_privileges(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<Vec<ColumnGrant>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    privilege::column_privileges(&handle, &database, oid).await
}

/// Los privilegios por omisión definidos en la base.
#[tauri::command]
pub async fn default_privileges(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
) -> Result<Vec<DefaultGrant>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    privilege::default_privileges(&handle, &database).await
}

/// Los privilegios que ya tiene un esquema.
#[tauri::command]
pub async fn schema_privileges(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<Vec<PrivilegeGrant>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    privilege::schema_privileges(&handle, &database, oid).await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn policy_preview(changes: Vec<PolicyChange>) -> Result<Vec<Statement>> {
    policy::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn policy_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<PolicyChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&policy::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        policy::apply(&handle, &database, &changes),
    )
    .await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn extension_preview(changes: Vec<ExtensionChange>) -> Result<Vec<Statement>> {
    extension::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn extension_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<ExtensionChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&extension::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        extension::apply(&handle, &database, &changes),
    )
    .await
}

/// La extensión instalada tal como está, para precargar el diálogo de edición.
#[tauri::command]
pub async fn extension_info(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    name: String,
) -> Result<ExtensionInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    extension::extension(&handle, &database, &name).await
}

/// Las extensiones que el paquete ofrece, para el selector al instalar.
#[tauri::command]
pub async fn available_extensions(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
) -> Result<Vec<AvailableExtension>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    extension::available(&handle, &database).await
}

// --- Datos externos: wrappers, servidores foráneos y mapeos de usuario ---

#[tauri::command]
pub fn fdw_preview(changes: Vec<FdwChange>) -> Result<Vec<Statement>> {
    fdw::fdw_statements(&changes)
}

#[tauri::command]
pub async fn fdw_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<FdwChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    let statements = fdw::fdw_statements(&changes)?;
    let sql = sql_of(&statements);
    record_applied(
        &state,
        id,
        &database,
        sql,
        fdw::apply(&handle, &database, &statements),
    )
    .await
}

#[tauri::command]
pub async fn fdw_info(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    name: String,
) -> Result<FdwInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    fdw::fdw_info(&handle, &database, &name).await
}

/// Los wrappers disponibles, para el selector al crear un servidor foráneo.
#[tauri::command]
pub async fn available_fdws(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
) -> Result<Vec<String>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    fdw::available_fdws(&handle, &database).await
}

#[tauri::command]
pub fn foreign_server_preview(changes: Vec<ServerChange>) -> Result<Vec<Statement>> {
    fdw::server_statements(&changes)
}

#[tauri::command]
pub async fn foreign_server_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<ServerChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    let statements = fdw::server_statements(&changes)?;
    let sql = sql_of(&statements);
    record_applied(
        &state,
        id,
        &database,
        sql,
        fdw::apply(&handle, &database, &statements),
    )
    .await
}

#[tauri::command]
pub async fn foreign_server_info(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    name: String,
) -> Result<ServerInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    fdw::server_info(&handle, &database, &name).await
}

#[tauri::command]
pub fn user_mapping_preview(changes: Vec<UserMappingChange>) -> Result<Vec<Statement>> {
    fdw::user_mapping_statements(&changes)
}

#[tauri::command]
pub async fn user_mapping_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<UserMappingChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    let statements = fdw::user_mapping_statements(&changes)?;
    let sql = sql_of(&statements);
    record_applied(
        &state,
        id,
        &database,
        sql,
        fdw::apply(&handle, &database, &statements),
    )
    .await
}

/// Los mapeos de usuario de un servidor foráneo, para la sección de su panel de detalle.
#[tauri::command]
pub async fn user_mappings(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    server: String,
) -> Result<Vec<UserMapping>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    fdw::user_mappings(&handle, &database, &server).await
}

/// El estado de Row-Level Security de una tabla: el interruptor y sus políticas.
#[tauri::command]
pub async fn table_security(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<TableSecurity> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    policy::table_security(&handle, &database, oid).await
}

/// El SQL que se ejecutaría sobre una secuencia, sin ejecutar nada.
#[tauri::command]
pub fn sequence_preview(changes: Vec<SequenceChange>) -> Result<Vec<Statement>> {
    sequence::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn sequence_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<SequenceChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&sequence::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        sequence::apply(&handle, &database, &changes),
    )
    .await
}

/// La definición de una secuencia, para precargar el formulario y mostrar su valor actual.
#[tauri::command]
pub async fn sequence_info(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<SequenceInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    sequence::info(&handle, &database, oid).await
}

/// El SQL que se ejecutaría sobre un tipo, sin ejecutar nada.
#[tauri::command]
pub fn type_preview(changes: Vec<TypeChange>) -> Result<Vec<Statement>> {
    types::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn type_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<TypeChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&types::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        types::apply(&handle, &database, &changes),
    )
    .await
}

/// La definición de un tipo: sus valores si es una enumeración, sus campos si es compuesto.
#[tauri::command]
pub async fn type_info(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<TypeInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    types::info(&handle, &database, oid).await
}

/// El SQL que se ejecutaría sobre un dominio, sin ejecutar nada.
#[tauri::command]
pub fn domain_preview(changes: Vec<DomainChange>) -> Result<Vec<Statement>> {
    domain::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn domain_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<DomainChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&domain::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        domain::apply(&handle, &database, &changes),
    )
    .await
}

/// La definición de un dominio: tipo base, `DEFAULT`, `NOT NULL` y restricciones.
#[tauri::command]
pub async fn domain_info(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<DomainInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    domain::info(&handle, &database, oid).await
}

/// El SQL que se ejecutaría sobre un esquema, sin ejecutar nada.
#[tauri::command]
pub fn schema_preview(changes: Vec<SchemaChange>) -> Result<Vec<Statement>> {
    schema::statements(&changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn schema_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<SchemaChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&schema::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        schema::apply(&handle, &database, &changes),
    )
    .await
}

/// El SQL que se ejecutaría sobre una base, sin ejecutar nada.
#[tauri::command]
pub fn database_preview(changes: Vec<DatabaseChange>) -> Result<Vec<Statement>> {
    database::statements(&changes)
}

/// Aplica los cambios pendientes.
///
/// A diferencia del resto del DDL, **sin transacción**: `CREATE DATABASE` y `DROP DATABASE` no
/// corren adentro de un bloque transaccional. El núcleo elige además desde qué base conectarse.
#[tauri::command]
pub async fn database_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    changes: Vec<DatabaseChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;

    database::apply(&handle, &changes).await
}

/// La definición de una base: dueño, codificación, locales, tamaño y límite de conexiones.
#[tauri::command]
pub async fn database_info(
    state: State<'_, AppState>,
    id: ProfileId,
    name: String,
) -> Result<DatabaseInfo> {
    let handle = state.manager.require(id).await?;

    database::info(&handle, &name).await
}

/// El SQL que se ejecutaría sobre una partición, sin ejecutar nada.
///
/// Pide el perfil, a diferencia de las demás vistas previas, porque `DETACH … CONCURRENTLY` no
/// existe antes de PostgreSQL 14 y la decisión sale de `ServerCaps`. Mismo caso que
/// `maintenance_plan`.
#[tauri::command]
pub async fn partition_preview(
    state: State<'_, AppState>,
    id: ProfileId,
    changes: Vec<PartitionChange>,
) -> Result<Vec<Statement>> {
    let handle = state.manager.require(id).await?;

    partition::statements(&changes, &handle.caps)
}

/// Aplica los cambios pendientes. Sin transacción si hay un `DETACH … CONCURRENTLY`.
#[tauri::command]
pub async fn partition_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<PartitionChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    partition::apply(&handle, &database, &changes).await
}

/// Cómo está particionada una tabla y qué particiones tiene.
#[tauri::command]
pub async fn table_partitions(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<PartitioningInfo> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    partition::info(&handle, &database, oid).await
}

/// El SQL del comentario, sin ejecutar nada.
#[tauri::command]
pub fn comment_preview(changes: Vec<CommentChange>) -> Result<Vec<Statement>> {
    comment::statements(&changes)
}

/// Aplica los comentarios pendientes en una sola transacción.
#[tauri::command]
pub async fn comment_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    changes: Vec<CommentChange>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let sql = sql_of(&comment::statements(&changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        comment::apply(&handle, &database, &changes),
    )
    .await
}
