//! Backups y restores.
//!
//! Mismo molde que el mantenimiento: un comando arma lo que se va a ejecutar sin ejecutarlo, otro
//! lo lanza y transmite el avance por un canal, y un tercero lo corta. La diferencia está en qué se
//! guarda para poder cortarlo — un `VACUUM` se cancela pidiéndoselo al servidor, pero acá la tarea
//! es un proceso hijo de la aplicación y lo que hace falta es el extremo del canal que le avisa.
//!
//! Backup y restore son la misma estructura con distinta carga, así que comparten [`ExternalTask`] y
//! se distinguen solo por el mapa donde se registran.

use pgforge_core::backup::restore::{self, RestoreOptions, RestorePlan};
use pgforge_core::backup::{self, BackupOptions, BackupPlan};
use pgforge_core::error::ErrorPayload;
use pgforge_core::{Error, ProfileId, Result};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, oneshot};

use crate::state::{AppState, ExternalTask};

/// La línea de comando que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub async fn backup_plan(
    state: State<'_, AppState>,
    id: ProfileId,
    options: BackupOptions,
) -> Result<BackupPlan> {
    let handle = state.manager.require(id).await?;
    backup::plan(&handle, &options).await
}

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BackupEvent {
    Started {
        command: Vec<String>,
    },
    Progress {
        message: String,
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

/// Lanza el backup y devuelve su identificador, con el que se lo puede cancelar.
#[tauri::command]
pub async fn backup_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    options: BackupOptions,
    channel: Channel<BackupEvent>,
) -> Result<String> {
    let handle = state.manager.require(id).await?;

    // El plan se pide antes de lanzar la tarea para que los errores que se pueden anticipar —una
    // combinación inválida, un pg_dump más viejo que el servidor— lleguen como el error de este
    // comando y no como un evento de una tarea que ya arrancó.
    let plan = backup::plan(&handle, &options).await?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut lines) = mpsc::channel::<String>(64);
    let task_id = uuid::Uuid::new_v4().to_string();

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            let _ = channel.send(BackupEvent::Started {
                command: plan.command,
            });

            // El avance se reenvía a medida que llega: leerlo al final sería una barra de progreso
            // falsa durante todo el backup.
            {
                let channel = channel.clone();
                tokio::spawn(async move {
                    while let Some(message) = lines.recv().await {
                        let _ = channel.send(BackupEvent::Progress { message });
                    }
                });
            }

            let event = match backup::run(&handle, &options, progress, cancelled).await {
                Ok(outcome) => BackupEvent::Finished {
                    path: outcome.path.display().to_string(),
                    bytes: outcome.bytes,
                    seconds: outcome.seconds,
                },
                Err(error) => BackupEvent::Failed {
                    error: ErrorPayload::from(&error),
                },
            };
            let _ = channel.send(event);

            app.state::<AppState>()
                .backups
                .lock()
                .await
                .remove(&task_id);
        })
    };

    state
        .backups
        .lock()
        .await
        .insert(task_id.clone(), ExternalTask { cancel });

    Ok(task_id)
}

/// Corta un backup en curso. El archivo a medio escribir lo borra el núcleo.
#[tauri::command]
pub async fn backup_cancel(state: State<'_, AppState>, task_id: String) -> Result<()> {
    let entry = state
        .backups
        .lock()
        .await
        .remove(&task_id)
        .ok_or_else(|| Error::Config("el backup ya no está en curso".to_owned()))?;

    // Si el otro extremo ya no está, la tarea terminó sola entre medio: no es un error.
    let _ = entry.cancel.send(());
    Ok(())
}

/// La línea de comando del restore que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub async fn restore_plan(
    state: State<'_, AppState>,
    id: ProfileId,
    options: RestoreOptions,
) -> Result<RestorePlan> {
    let handle = state.manager.require(id).await?;
    restore::plan(&handle, &options).await
}

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RestoreEvent {
    Started {
        command: Vec<String>,
    },
    Progress {
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    Finished {
        database: String,
        seconds: f64,
        ignored_errors: u64,
    },
    Failed {
        error: ErrorPayload,
    },
}

/// Lanza el restore y devuelve su identificador, con el que se lo puede cancelar.
#[tauri::command]
pub async fn restore_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    options: RestoreOptions,
    channel: Channel<RestoreEvent>,
) -> Result<String> {
    let handle = state.manager.require(id).await?;

    // El plan se pide antes de lanzar la tarea para que los errores que se pueden anticipar —una
    // combinación inválida, un pg_restore más viejo que el servidor— lleguen como el error de este
    // comando y no como un evento de una tarea que ya arrancó.
    let plan = restore::plan(&handle, &options).await?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut lines) = mpsc::channel::<String>(64);
    let task_id = uuid::Uuid::new_v4().to_string();

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            let _ = channel.send(RestoreEvent::Started {
                command: plan.command,
            });

            {
                let channel = channel.clone();
                tokio::spawn(async move {
                    while let Some(message) = lines.recv().await {
                        let _ = channel.send(RestoreEvent::Progress { message });
                    }
                });
            }

            let event = match restore::run(&handle, &options, progress, cancelled).await {
                Ok(outcome) => RestoreEvent::Finished {
                    database: outcome.database,
                    seconds: outcome.seconds,
                    ignored_errors: outcome.ignored_errors,
                },
                Err(error) => RestoreEvent::Failed {
                    error: ErrorPayload::from(&error),
                },
            };
            let _ = channel.send(event);

            app.state::<AppState>()
                .restores
                .lock()
                .await
                .remove(&task_id);
        })
    };

    state
        .restores
        .lock()
        .await
        .insert(task_id.clone(), ExternalTask { cancel });

    Ok(task_id)
}

/// Corta un restore en curso.
#[tauri::command]
pub async fn restore_cancel(state: State<'_, AppState>, task_id: String) -> Result<()> {
    let entry = state
        .restores
        .lock()
        .await
        .remove(&task_id)
        .ok_or_else(|| Error::Config("el restore ya no está en curso".to_owned()))?;

    // Si el otro extremo ya no está, la tarea terminó sola entre medio: no es un error.
    let _ = entry.cancel.send(());
    Ok(())
}
