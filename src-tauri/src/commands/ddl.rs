//! Estructura de tablas: crear, cambiar y borrar tablas, columnas, índices y constraints.

use pgforge_core::ddl::index::{self, IndexDef, IndexInfo};
use pgforge_core::ddl::table::{self, ConstraintInfo, Statement, TableChange};
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
