//! Backups y restores.
//!
//! Mismo molde que el mantenimiento: un comando arma lo que se va a ejecutar sin ejecutarlo y otro
//! lo lanza y va anotando su avance en el registro de procesos. La diferencia está en cómo se corta
//! — un `VACUUM` se cancela pidiéndoselo al servidor, pero acá la tarea es un proceso hijo de la
//! aplicación y lo que hace falta es el extremo del canal que le avisa, que es lo que guarda
//! `Cancel::Child`.
//!
//! Backup y restore son la misma estructura con distinta carga, y se distinguen solo por el
//! `ProcessKind` con el que se anotan.

use pgforge_core::backup::restore::{self, RestoreOptions, RestorePlan};
use pgforge_core::backup::{self, BackupOptions, BackupPlan};
use pgforge_core::error::ErrorPayload;
use pgforge_core::{ProfileId, Result};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, oneshot};

use crate::process::{Cancel, Outcome, ProcessKind};
use crate::state::AppState;

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

/// Lanza el backup y devuelve su identificador, con el que se lo puede cancelar.
#[tauri::command]
pub async fn backup_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    options: BackupOptions,
) -> Result<String> {
    let handle = state.manager.require(id).await?;

    // El plan se pide antes de lanzar la tarea para que los errores que se pueden anticipar —una
    // combinación inválida, un pg_dump más viejo que el servidor— lleguen como el error de este
    // comando y no como un evento de una tarea que ya arrancó.
    let plan = backup::plan(&handle, &options).await?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut lines) = mpsc::channel::<String>(64);

    let task_id = state
        .processes
        .start(
            ProcessKind::Backup,
            id,
            options.database.clone(),
            options.path.display().to_string(),
            plan.command.join(" "),
            Cancel::Child(cancel),
        )
        .await;

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            // El avance se anota a medida que llega: leerlo al final sería una barra de progreso
            // falsa durante todo el backup.
            {
                let app = app.clone();
                let task_id = task_id.clone();
                tokio::spawn(async move {
                    let state = app.state::<AppState>();
                    while let Some(message) = lines.recv().await {
                        state.processes.log(&task_id, message).await;
                    }
                });
            }

            let result = backup::run(&handle, &options, progress, cancelled).await;

            let state = app.state::<AppState>();
            match result {
                Ok(outcome) => {
                    state
                        .processes
                        .finish(
                            &task_id,
                            Outcome {
                                seconds: outcome.seconds,
                                bytes: Some(outcome.bytes),
                                path: Some(outcome.path.display().to_string()),
                                ..Outcome::default()
                            },
                        )
                        .await
                }
                Err(error) => {
                    state
                        .processes
                        .fail(&task_id, ErrorPayload::from(&error))
                        .await
                }
            }
        })
    };

    Ok(task_id)
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

/// Lanza el restore y devuelve su identificador, con el que se lo puede cancelar.
#[tauri::command]
pub async fn restore_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    options: RestoreOptions,
) -> Result<String> {
    let handle = state.manager.require(id).await?;

    // El plan se pide antes de lanzar la tarea para que los errores que se pueden anticipar —una
    // combinación inválida, un pg_restore más viejo que el servidor— lleguen como el error de este
    // comando y no como un evento de una tarea que ya arrancó.
    let plan = restore::plan(&handle, &options).await?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut lines) = mpsc::channel::<String>(64);

    let task_id = state
        .processes
        .start(
            ProcessKind::Restore,
            id,
            options.database.clone(),
            options.source.display().to_string(),
            plan.command.join(" "),
            Cancel::Child(cancel),
        )
        .await;

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            {
                let app = app.clone();
                let task_id = task_id.clone();
                tokio::spawn(async move {
                    let state = app.state::<AppState>();
                    while let Some(message) = lines.recv().await {
                        state.processes.log(&task_id, message).await;
                    }
                });
            }

            let result = restore::run(&handle, &options, progress, cancelled).await;

            let state = app.state::<AppState>();
            match result {
                Ok(outcome) => {
                    state
                        .processes
                        .finish(
                            &task_id,
                            Outcome {
                                seconds: outcome.seconds,
                                ignored_errors: Some(outcome.ignored_errors),
                                database: Some(outcome.database),
                                ..Outcome::default()
                            },
                        )
                        .await
                }
                Err(error) => {
                    state
                        .processes
                        .fail(&task_id, ErrorPayload::from(&error))
                        .await
                }
            }
        })
    };

    Ok(task_id)
}
