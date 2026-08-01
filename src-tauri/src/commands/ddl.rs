//! Estructura de tablas: crear, cambiar y borrar tablas y columnas.

use pgforge_core::ddl::table::{self, Statement, TableChange};
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
