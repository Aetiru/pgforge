//! Datos de una tabla.
//!
//! La forma de la tabla viaja de ida y de vuelta en vez de guardarse en el estado: describe el
//! catálogo, no una conexión ni una transacción abierta, y así la interfaz puede mostrar la misma
//! grilla después de un refresco sin que el backend tenga que recordar nada entre llamadas.

use std::path::PathBuf;
use std::time::Instant;

use pgforge_core::data::{
    self, Applied, Change, CopyCommand, Cursor, ExportSpec, ImportSpec, Page, Statement, TableShape,
};
use pgforge_core::error::ErrorPayload;
use pgforge_core::{Error, ProfileId, Result};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, oneshot};

use crate::state::{AppState, ExternalTask};

/// Columnas y clave de una tabla. Es lo que decide si la grilla se abre editable.
#[tauri::command]
pub async fn data_open(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<TableShape> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::shape(&handle, &database, oid).await
}

/// Una página de filas. Con `cursor` en `null` devuelve la primera.
#[tauri::command]
pub async fn data_page(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    shape: TableShape,
    cursor: Option<Cursor>,
    limit: Option<usize>,
) -> Result<Page> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::page(
        &handle,
        &database,
        &shape,
        cursor.as_ref(),
        limit.unwrap_or(data::DEFAULT_PAGE_SIZE),
    )
    .await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
///
/// No toca la red ni el estado: es el núcleo puro expuesto, para que la vista previa muestre
/// exactamente lo que se va a correr y no una reconstrucción parecida.
#[tauri::command]
pub fn data_preview(shape: TableShape, changes: Vec<Change>) -> Result<Vec<Statement>> {
    data::statements(&shape, &changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn data_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    shape: TableShape,
    changes: Vec<Change>,
) -> Result<Applied> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::apply(&handle, &database, &shape, &changes).await
}

// --------------------------------------------------------------------------
// Exportar e importar con COPY. Mismo molde que backup/restore: un comando arma
// el COPY sin ejecutarlo, otro lo lanza y transmite el avance por un canal, y un
// tercero lo corta avisándole a la tarea que suelte el sink o cancele el stream.
// --------------------------------------------------------------------------

/// El `COPY ... TO STDOUT` que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn data_export_preview(spec: ExportSpec) -> Result<CopyCommand> {
    data::export_command(&spec)
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ExportEvent {
    Started {
        command: String,
    },
    Progress {
        bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        path: String,
        bytes: u64,
        seconds: f64,
    },
    Failed {
        error: ErrorPayload,
    },
}

/// Lanza la exportación y devuelve su identificador, con el que se la puede cancelar.
#[tauri::command]
pub async fn data_export_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    spec: ExportSpec,
    path: String,
    channel: Channel<ExportEvent>,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    // Se arma el COPY antes de lanzar la tarea para que una combinación de opciones inválida llegue
    // como el error de este comando y no como un evento de una tarea que ya arrancó.
    let command = data::export_command(&spec)?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut bytes_rx) = mpsc::channel::<u64>(64);
    let task_id = uuid::Uuid::new_v4().to_string();
    let path = PathBuf::from(path);

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            let _ = channel.send(ExportEvent::Started {
                command: command.sql,
            });

            // El avance se reenvía a medida que llega: leerlo al final sería una barra falsa durante
            // toda la exportación.
            {
                let channel = channel.clone();
                tokio::spawn(async move {
                    while let Some(bytes) = bytes_rx.recv().await {
                        let _ = channel.send(ExportEvent::Progress { bytes });
                    }
                });
            }

            let started = Instant::now();
            let event =
                match data::export_to_file(&handle, &database, &spec, &path, progress, cancelled)
                    .await
                {
                    Ok(outcome) => ExportEvent::Finished {
                        path: path.display().to_string(),
                        bytes: outcome.bytes,
                        seconds: started.elapsed().as_secs_f64(),
                    },
                    Err(error) => ExportEvent::Failed {
                        error: ErrorPayload::from(&error),
                    },
                };
            let _ = channel.send(event);

            app.state::<AppState>().copies.lock().await.remove(&task_id);
        });
    }

    state
        .copies
        .lock()
        .await
        .insert(task_id.clone(), ExternalTask { cancel });

    Ok(task_id)
}

/// El `COPY ... FROM STDIN` que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn data_import_preview(spec: ImportSpec) -> Result<CopyCommand> {
    data::import_command(&spec)
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ImportEvent {
    Started {
        command: String,
    },
    Progress {
        bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        bytes: u64,
        rows: u64,
        seconds: f64,
    },
    Failed {
        error: ErrorPayload,
    },
}

/// Lanza la importación y devuelve su identificador, con el que se la puede cancelar.
#[tauri::command]
pub async fn data_import_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    spec: ImportSpec,
    path: String,
    channel: Channel<ImportEvent>,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let command = data::import_command(&spec)?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut bytes_rx) = mpsc::channel::<u64>(64);
    let task_id = uuid::Uuid::new_v4().to_string();
    let path = PathBuf::from(path);

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            let _ = channel.send(ImportEvent::Started {
                command: command.sql,
            });

            {
                let channel = channel.clone();
                tokio::spawn(async move {
                    while let Some(bytes) = bytes_rx.recv().await {
                        let _ = channel.send(ImportEvent::Progress { bytes });
                    }
                });
            }

            let started = Instant::now();
            let event =
                match data::import_from_file(&handle, &database, &spec, &path, progress, cancelled)
                    .await
                {
                    Ok(outcome) => ImportEvent::Finished {
                        bytes: outcome.bytes,
                        // En importación el núcleo siempre trae las filas; el `unwrap_or(0)` es solo
                        // por completar el tipo, no un caso que pueda darse.
                        rows: outcome.rows.unwrap_or(0),
                        seconds: started.elapsed().as_secs_f64(),
                    },
                    Err(error) => ImportEvent::Failed {
                        error: ErrorPayload::from(&error),
                    },
                };
            let _ = channel.send(event);

            app.state::<AppState>().copies.lock().await.remove(&task_id);
        });
    }

    state
        .copies
        .lock()
        .await
        .insert(task_id.clone(), ExternalTask { cancel });

    Ok(task_id)
}

/// Corta una exportación o importación en curso.
#[tauri::command]
pub async fn data_copy_cancel(state: State<'_, AppState>, task_id: String) -> Result<()> {
    let entry = state
        .copies
        .lock()
        .await
        .remove(&task_id)
        .ok_or_else(|| Error::Config("la transferencia ya no está en curso".to_owned()))?;

    // Si el otro extremo ya no está, la tarea terminó sola entre medio: no es un error.
    let _ = entry.cancel.send(());
    Ok(())
}
