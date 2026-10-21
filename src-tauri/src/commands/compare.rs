//! Comparación de esquemas entre dos servidores, o entre uno y una instantánea guardada.

use pgforge_core::compare::data::{self, DataComparison};
use pgforge_core::compare::{self, Comparison};
use pgforge_core::conn::CancelSink;
use pgforge_core::introspect;
use pgforge_core::{ProfileId, Result};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::state::{AppState, ReadEntry};

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

/// Guarda el esquema de un lado en un archivo, para compararlo más adelante contra lo que haya.
#[tauri::command]
pub async fn schema_snapshot_save(
    state: State<'_, AppState>,
    side: CompareSide,
    path: String,
) -> Result<()> {
    let path = state.paths.check(&path)?;
    let handle = state.manager.require(side.id).await?;
    compare::file::save(&handle, &side.database, &side.schema, &path).await
}

/// Compara una instantánea guardada (origen) contra un esquema en vivo (destino).
#[tauri::command]
pub async fn schema_compare_file(
    state: State<'_, AppState>,
    path: String,
    target: CompareSide,
) -> Result<Comparison> {
    let source = compare::file::load(&state.paths.check(&path)?)?;
    let handle = state.manager.require(target.id).await?;
    compare::compare_with_file(&source, &handle, &target.database, &target.schema).await
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

/// Una tabla de un lado de la comparación de datos.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSide {
    pub id: ProfileId,
    pub database: String,
    pub schema: String,
    pub table: String,
}

/// Avance de la lectura, por el canal.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DataCompareEvent {
    Progress { source_rows: u64, target_rows: u64 },
}

/// Compara el contenido de dos tablas y devuelve informe y script juntos.
///
/// Como la comparación de esquemas, no tiene un `apply`: el script se copia o se abre en una pestaña
/// de consulta contra el destino. `request_id` registra las dos lecturas (una por servidor) bajo
/// `{request_id}:s` y `{request_id}:t`, para que `data_compare_cancel` las pueda abortar del lado
/// del servidor.
#[tauri::command]
pub async fn data_compare(
    state: State<'_, AppState>,
    source: DataSide,
    target: DataSide,
    max_diffs: Option<usize>,
    request_id: String,
    channel: Channel<DataCompareEvent>,
) -> Result<DataComparison> {
    let source_handle = state.manager.require(source.id).await?;
    let target_handle = state.manager.require(target.id).await?;

    let (source_sink, target_sink) = (CancelSink::new(), CancelSink::new());
    {
        let mut reads = state.reads.lock().await;
        reads.insert(
            format!("{request_id}:s"),
            ReadEntry {
                profile: source.id,
                sink: source_sink.clone(),
            },
        );
        reads.insert(
            format!("{request_id}:t"),
            ReadEntry {
                profile: target.id,
                sink: target_sink.clone(),
            },
        );
    }

    let outcome = data::compare(
        data::Side {
            handle: &source_handle,
            database: &source.database,
            schema: &source.schema,
            table: &source.table,
            sink: Some(source_sink),
        },
        data::Side {
            handle: &target_handle,
            database: &target.database,
            schema: &target.schema,
            table: &target.table,
            sink: Some(target_sink),
        },
        max_diffs.unwrap_or(data::DEFAULT_MAX_DIFFS),
        &|progress| {
            let _ = channel.send(DataCompareEvent::Progress {
                source_rows: progress.source_rows,
                target_rows: progress.target_rows,
            });
        },
    )
    .await;

    // Se sacan pase lo que pase: una entrada olvidada dejaría que una cancelación tardía le pegara
    // a una conexión que el pool ya le prestó a otra cosa.
    let mut reads = state.reads.lock().await;
    reads.remove(&format!("{request_id}:s"));
    reads.remove(&format!("{request_id}:t"));
    outcome
}

/// Aborta una comparación de datos en curso, en los dos servidores. Sin nada registrado no hace
/// nada: ya terminó, y eso no es un error.
#[tauri::command]
pub async fn data_compare_cancel(state: State<'_, AppState>, request_id: String) -> Result<()> {
    for suffix in ["s", "t"] {
        let entry = state
            .reads
            .lock()
            .await
            .remove(&format!("{request_id}:{suffix}"));
        let Some(entry) = entry else { continue };
        let handle = state.manager.require(entry.profile).await?;
        for token in entry.sink.tokens() {
            handle.cancel(&token).await?;
        }
    }
    Ok(())
}
