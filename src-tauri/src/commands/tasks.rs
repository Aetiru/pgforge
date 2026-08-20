//! Sentencias largas que corren solas mientras se sigue usando la aplicación.
//!
//! Un `VACUUM FULL` o un `CREATE INDEX CONCURRENTLY` tardan minutos u horas. Antes cada uno vivía
//! adentro del diálogo que lo lanzaba, así que la ventana quedaba tomada hasta que terminara: la
//! única forma de mirar otra tabla mientras tanto era cancelar. Acá la tarea se lanza, se registra
//! por identificador y su avance viaja por un canal; el diálogo se cierra y la interfaz la sigue
//! desde la vista de procesos.
//!
//! Lo que corre es lo que arma el núcleo —`maintenance::statement`, `index::create_sql`—: acá no se
//! escribe SQL, solo se lo pone a correr sobre una sesión propia y se reporta.

use std::time::Instant;

use pgforge_core::error::ErrorPayload;
use pgforge_core::{Error, ProfileId, Result};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::state::{AppState, TaskEntry};

/// Lo que la interfaz recibe mientras la tarea corre.
///
/// Es el mismo juego de eventos para el mantenimiento y para la creación de un índice: las dos son
/// una sentencia que empieza, cuenta cosas por `NOTICE` y termina bien o mal.
#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskEvent {
    Started { sql: String },
    Notice { severity: String, message: String },
    Finished { seconds: f64 },
    Failed { error: ErrorPayload },
}

/// Pone a correr `sql` en una sesión propia y devuelve el identificador con el que se lo cancela.
///
/// Sin `statement_timeout`: el del perfil está pensado para consultas y mataría un `VACUUM` a los
/// treinta segundos. La sesión es dedicada por dos razones —se la puede cancelar, y una tarea de
/// diez minutos no le saca una conexión del pool al explorador—.
pub async fn spawn_statement(
    app: AppHandle,
    state: &AppState,
    id: ProfileId,
    database: String,
    sql: String,
    channel: Channel<TaskEvent>,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let mut session = handle.open_session(&database, None).await?;
    let notices = session.take_notices();
    let cancel = session.cancel_token();
    let task_id = uuid::Uuid::new_v4().to_string();

    {
        let sql = sql.clone();
        let task_id = task_id.clone();
        tokio::spawn(async move {
            let _ = channel.send(TaskEvent::Started { sql: sql.clone() });

            // Los NOTICE llegan mientras la sentencia corre, así que hay que escucharlos en
            // paralelo: si se leyeran después, el avance aparecería todo junto al final.
            if let Some(mut notices) = notices {
                let channel = channel.clone();
                tokio::spawn(async move {
                    while let Some(notice) = notices.recv().await {
                        let _ = channel.send(TaskEvent::Notice {
                            severity: notice.severity,
                            message: notice.message,
                        });
                    }
                });
            }

            let started = Instant::now();
            let event = match session.execute_batch(&sql).await {
                Ok(()) => TaskEvent::Finished {
                    seconds: started.elapsed().as_secs_f64(),
                },
                Err(error) => TaskEvent::Failed {
                    error: ErrorPayload::from(&error),
                },
            };
            let _ = channel.send(event);

            app.state::<AppState>().tasks.lock().await.remove(&task_id);
        })
    };

    state.tasks.lock().await.insert(
        task_id.clone(),
        TaskEntry {
            profile: id,
            cancel,
        },
    );

    Ok(task_id)
}

/// Cancela una sentencia larga en curso.
#[tauri::command]
pub async fn task_cancel(state: State<'_, AppState>, task_id: String) -> Result<()> {
    let (profile, cancel) = {
        let tasks = state.tasks.lock().await;
        let entry = tasks
            .get(&task_id)
            .ok_or_else(|| Error::Config("la tarea ya no está en curso".to_owned()))?;
        (entry.profile, entry.cancel.clone())
    };

    // Se le pide al servidor que aborte, en vez de abortar la tarea de Rust: matar la tarea local
    // dejaría al servidor terminando el VACUUM sin que nadie lo esté mirando.
    let handle = state.manager.require(profile).await?;
    handle.cancel(&cancel).await
}
