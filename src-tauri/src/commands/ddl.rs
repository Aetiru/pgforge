//! Estructura de tablas: crear, cambiar y borrar tablas, columnas, índices, constraints, vistas,
//! funciones, triggers, roles, privilegios y políticas de seguridad por fila.

use pgforge_core::ddl::extension::{self, AvailableExtension, ExtensionChange, ExtensionInfo};
use pgforge_core::ddl::function;
use pgforge_core::ddl::index::{self, IndexDef, IndexInfo};
use pgforge_core::ddl::policy::{self, PolicyChange, TableSecurity};
use pgforge_core::ddl::privilege::{
    self, ColumnGrant, DefaultGrant, PrivilegeChange, PrivilegeGrant,
};
use pgforge_core::ddl::role::{self, RoleChange, RoleInfo};
use pgforge_core::ddl::table::{self, ConstraintInfo, Statement, TableChange};
use pgforge_core::ddl::trigger::{self, TriggerChange, TriggerInfo};
use pgforge_core::ddl::view::{self, ViewChange};
use pgforge_core::{ProfileId, Result};
use tauri::State;

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

    table::apply(&handle, &database, &changes).await
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

/// Crea un índice. Nunca en transacción: es lo que permite `CONCURRENTLY` (ver `ddl::index`).
#[tauri::command]
pub async fn index_create(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    def: IndexDef,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    index::create(&handle, &database, &def).await
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

    view::apply(&handle, &database, &changes).await
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

    trigger::apply(&handle, &database, &changes).await
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

    role::apply(&handle, &database, &changes).await
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

    privilege::apply(&handle, &database, &changes).await
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

    policy::apply(&handle, &database, &changes).await
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

    extension::apply(&handle, &database, &changes).await
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
