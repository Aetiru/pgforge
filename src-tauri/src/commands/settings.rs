//! Configuración del servidor: leer y cambiar `pg_settings`.

use pgforge_core::ddl::table::Statement;
use pgforge_core::settings::{self, Setting, SettingChange};
use pgforge_core::{ProfileId, Result};
use tauri::State;

use crate::commands::{record_applied, sql_of};
use crate::state::AppState;

#[tauri::command]
pub async fn server_settings(state: State<'_, AppState>, id: ProfileId) -> Result<Vec<Setting>> {
    let handle = state.manager.require(id).await?;
    settings::list(&handle).await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn settings_preview(changes: Vec<SettingChange>) -> Vec<Statement> {
    settings::statements(&changes)
}

/// Aplica los cambios y recarga. Devuelve `true` si algo quedó pendiente de reinicio.
#[tauri::command]
pub async fn settings_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    changes: Vec<SettingChange>,
) -> Result<bool> {
    let handle = state.manager.require(id).await?;
    // Un `ALTER SYSTEM` es del servidor y no de una base: se anota contra la de mantenimiento, que
    // es por donde entró.
    let database = handle.default_database().to_owned();

    let sql = sql_of(&settings::statements(&changes));
    record_applied(
        &state,
        id,
        &database,
        sql,
        settings::apply(&handle, &changes),
    )
    .await
}
