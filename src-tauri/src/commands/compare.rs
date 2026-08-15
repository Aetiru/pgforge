//! Comparación de esquemas entre dos servidores.

use pgforge_core::compare::{self, Comparison};
use pgforge_core::introspect;
use pgforge_core::{ProfileId, Result};
use serde::Deserialize;
use tauri::State;

use crate::state::AppState;

/// Un lado de la comparación: qué esquema, de qué base, de qué servidor conectado.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSide {
    pub id: ProfileId,
    pub database: String,
    pub schema: String,
}

/// Compara dos esquemas y devuelve el informe junto con el SQL que sincronizaría el destino.
///
/// Los dos servidores tienen que estar conectados: la comparación lee en vivo de cada lado, y
/// conectar por su cuenta pediría contraseñas fuera del único lugar donde se piden.
#[tauri::command]
pub async fn schema_compare(
    state: State<'_, AppState>,
    source: CompareSide,
    target: CompareSide,
) -> Result<Comparison> {
    let source_handle = state.manager.require(source.id).await?;
    let target_handle = state.manager.require(target.id).await?;

    compare::compare(
        &source_handle,
        &source.database,
        &source.schema,
        &target_handle,
        &target.database,
        &target.schema,
    )
    .await
}

/// Los esquemas de una base, para elegir contra cuál comparar.
///
/// El árbol ya los trae, pero solo del servidor que uno abrió: el otro lado de la comparación puede
/// ser un servidor recién conectado y sin ninguna rama desplegada.
#[tauri::command]
pub async fn schema_names(
    state: State<'_, AppState>,
    id: ProfileId,
    database: String,
) -> Result<Vec<String>> {
    let handle = state.manager.require(id).await?;
    introspect::schema_names(&handle, &database).await
}
