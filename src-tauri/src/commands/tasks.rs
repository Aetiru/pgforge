//! Los procesos largos que corren solos mientras se sigue usando la aplicación.
//!
//! Un `VACUUM FULL` o un `CREATE INDEX CONCURRENTLY` tardan minutos u horas. Antes cada uno vivía
//! adentro del diálogo que lo lanzaba, así que la ventana quedaba tomada hasta que terminara: la
//! única forma de mirar otra tabla mientras tanto era cancelar. Acá la tarea se lanza, se anota en
//! el registro de [`crate::process`] y su avance viaja por el canal único de procesos; el diálogo
//! se cierra y la interfaz la sigue desde la vista de procesos.
//!
//! Lo que corre es lo que arma el núcleo —`maintenance::statement`, `index::create_sql`—: acá no se
//! escribe SQL, solo se lo pone a correr sobre una sesión propia y se reporta.

use std::time::Instant;

use pgforge_core::error::ErrorPayload;
use pgforge_core::{Error, ProfileId, Result};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::process::{Cancel, Outcome, ProcessEvent, ProcessKind};
use crate::state::AppState;

/// Engancha la ventana al registro de procesos.
///
/// Se llama una sola vez al arrancar la interfaz, y de nuevo cada vez que la ventana se recarga: el
/// primer mensaje que llega es el estado completo de lo que hay, así que reengancharse y ponerse al
/// día son la misma operación. Es lo que hace que recargar no pierda de vista un backup a medio
/// correr —ni el resultado de uno que terminó justo mientras no había nadie escuchando—.
#[tauri::command]
pub async fn process_watch(
    state: State<'_, AppState>,
    channel: Channel<ProcessEvent>,
) -> Result<()> {
    state.processes.watch(channel).await;
    Ok(())
}

/// Corta un proceso en curso, sea del servidor o un proceso hijo.
///
/// Cuál de los dos es lo sabe el registro, así que hay un solo comando: antes había uno por clase y
/// los cuatro decían lo mismo con distinto nombre.
#[tauri::command]
pub async fn process_cancel(state: State<'_, AppState>, task_id: String) -> Result<()> {
    let cancel = state
        .processes
        .take_cancel(&task_id)
        .await
        .ok_or_else(|| Error::Config("el proceso ya no está en curso".to_owned()))?;

    match cancel {
        // Se le pide al servidor que aborte, en vez de abortar la tarea de Rust: matar la tarea
        // local dejaría al servidor terminando el VACUUM sin que nadie lo esté mirando.
        Cancel::Server { profile, token } => {
            let handle = state.manager.require(profile).await?;
            handle.cancel(&token).await
        }
        // Si el otro extremo ya no está, la tarea terminó sola entre medio: no es un error. Qué
        // limpiar lo resuelve el núcleo —el backup borra su archivo a medio escribir—.
        Cancel::Child(sender) => {
            let _ = sender.send(());
            Ok(())
        }
    }
}

/// Saca de la lista un proceso terminado.
#[tauri::command]
pub async fn process_remove(state: State<'_, AppState>, task_id: String) -> Result<()> {
    state.processes.remove(&task_id).await;
    Ok(())
}

/// Saca de la lista todos los terminados. Los que siguen corriendo no se tocan.
#[tauri::command]
pub async fn process_clear(state: State<'_, AppState>) -> Result<()> {
    state.processes.clear_finished().await;
    Ok(())
}

/// Pone a correr `sql` en una sesión propia y devuelve el identificador con el que se lo cancela.
///
/// Sin `statement_timeout`: el del perfil está pensado para consultas y mataría un `VACUUM` a los
/// treinta segundos. La sesión es dedicada por dos razones —se la puede cancelar, y una tarea de
/// diez minutos no le saca una conexión del pool al explorador—.
pub async fn spawn_statement(
    app: AppHandle,
    state: &AppState,
    kind: ProcessKind,
    id: ProfileId,
    database: String,
    target: String,
    sql: String,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let mut session = handle.open_session(&database, None).await?;
    let notices = session.take_notices();
    let token = session.cancel_token();

    let task_id = state
        .processes
        .start(
            kind,
            id,
            database,
            target,
            sql.clone(),
            Cancel::Server { profile: id, token },
        )
        .await;

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            // Los NOTICE llegan mientras la sentencia corre, así que hay que escucharlos en
            // paralelo: si se leyeran después, el avance aparecería todo junto al final.
            if let Some(mut notices) = notices {
                let app = app.clone();
                let task_id = task_id.clone();
                tokio::spawn(async move {
                    let state = app.state::<AppState>();
                    while let Some(notice) = notices.recv().await {
                        state
                            .processes
                            .log(&task_id, format!("{}: {}", notice.severity, notice.message))
                            .await;
                    }
                });
            }

            let started = Instant::now();
            let result = session.execute_batch(&sql).await;

            let state = app.state::<AppState>();
            match result {
                Ok(()) => {
                    state
                        .processes
                        .finish(
                            &task_id,
                            Outcome {
                                seconds: started.elapsed().as_secs_f64(),
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
