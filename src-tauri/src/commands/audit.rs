//! Registro de auditoría: el historial con filtros, y su exportación a CSV o JSON.
//!
//! La consulta y el formato de los archivos son de `pgforge_core::sql::audit`; acá solo se traduce
//! y se verifica que el archivo de salida sea una ruta que el usuario eligió en un diálogo.

use pgforge_core::sql::{AuditFacets, AuditFilter, AuditPage, ExportFormat};
use pgforge_core::Result;
use tauri::State;

use crate::state::AppState;

/// Cuántas filas trae una página si la interfaz no dice. El total viaja aparte.
const DEFAULT_PAGE: i64 = 200;

#[tauri::command]
pub async fn audit_query(
    state: State<'_, AppState>,
    filter: AuditFilter,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<AuditPage> {
    state.history.lock().await.audit(
        &filter,
        limit.unwrap_or(DEFAULT_PAGE),
        offset.unwrap_or_default(),
    )
}

/// Los servidores y las bases que aparecen en el historial, para los selectores del filtro.
#[tauri::command]
pub async fn audit_facets(state: State<'_, AppState>) -> Result<AuditFacets> {
    state.history.lock().await.audit_facets()
}

/// Escribe en `path` todo lo que cumple el filtro (no solo la página a la vista) y devuelve cuántas
/// filas fueron.
#[tauri::command]
pub async fn audit_export(
    state: State<'_, AppState>,
    filter: AuditFilter,
    format: ExportFormat,
    path: String,
) -> Result<usize> {
    let path = state.paths.check(&path)?;
    state
        .history
        .lock()
        .await
        .audit_export(&filter, format, &path)
}
