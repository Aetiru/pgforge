//! Comandos del editor de SQL: revisar sin ejecutar, ayudas al escribir y versiones del texto.
//!
//! Acá no hay lógica: cada comando traduce argumentos y delega en `pgforge-core`.

use pgforge_core::introspect::{self, ColumnValues, RoutineSignature};
use pgforge_core::sql::{Check, VersionMeta};
use pgforge_core::{Error, ProfileId, Result};
use tauri::State;

use crate::state::AppState;

/// Revisa la sentencia con el servidor sin ejecutarla (ver `QuerySession::check`).
///
/// **No espera** el cerrojo de la sesión: si la pestaña está ejecutando algo, el cerrojo está
/// tomado por esa ejecución, y una revisión que esperara a que termine llegaría tarde y
/// mostraría un error sobre un texto que ya cambió. En ese caso se saltea.
#[tauri::command]
pub async fn query_check(state: State<'_, AppState>, tab_id: String, sql: String) -> Result<Check> {
    let session = {
        let tabs = state.queries.lock().await;
        tabs.get(&tab_id)
            .ok_or_else(|| Error::Config("la pestaña de consulta ya no está abierta".to_owned()))?
            .session
            .clone()
    };
    let Ok(session) = session.try_lock() else {
        return Ok(Check::Skipped {
            reason: "ocupada".to_owned(),
        });
    };
    session.check(&sql).await
}

/// Valores probables de una columna (etiquetas de un enum o los más frecuentes según las
/// estadísticas). Por el pool de esa base, no por la sesión de ninguna pestaña: no depende de su
/// transacción ni de su `search_path`, y no puede quedar detrás de una consulta en curso.
#[tauri::command]
pub async fn column_values(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    schema: String,
    table: String,
    column: String,
) -> Result<ColumnValues> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    let client = handle.client(&database).await?;
    introspect::column_values(&client, &schema, &table, &column, 30).await
}

/// Las sobrecargas de una función o procedimiento, para la ayuda de parámetros.
#[tauri::command]
pub async fn routine_signatures(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    schema: Option<String>,
    name: String,
) -> Result<Vec<RoutineSignature>> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    let client = handle.client(&database).await?;
    introspect::routine_signatures(&client, schema.as_deref(), &name).await
}

/// Consultas ya ejecutadas con éxito que empiezan con `prefix`, para completar en gris.
#[tauri::command]
pub async fn history_suggest(
    state: State<'_, AppState>,
    prefix: String,
    limit: Option<i64>,
) -> Result<Vec<String>> {
    state
        .history
        .lock()
        .await
        .suggest(&prefix, limit.unwrap_or(3).clamp(1, 20))
}

/// El reloj lo pone quien tiene el sistema operativo, no el núcleo (igual que en el historial).
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Guarda una copia del texto de la pestaña `key`. Devuelve su id, o `null` si no hacía falta.
#[tauri::command]
pub async fn version_save(
    state: State<'_, AppState>,
    key: String,
    text: String,
) -> Result<Option<i64>> {
    state.versions.lock().await.save(&key, &text, now_secs())
}

#[tauri::command]
pub async fn version_list(state: State<'_, AppState>, key: String) -> Result<Vec<VersionMeta>> {
    state.versions.lock().await.list(&key)
}

#[tauri::command]
pub async fn version_get(state: State<'_, AppState>, id: i64) -> Result<Option<String>> {
    state.versions.lock().await.get(id)
}

#[tauri::command]
pub async fn version_clear(state: State<'_, AppState>, key: String) -> Result<()> {
    state.versions.lock().await.clear(&key)
}
