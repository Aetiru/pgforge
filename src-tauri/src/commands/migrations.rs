//! Migraciones versionadas: guardar el script como archivo numerado, ver qué falta y aplicarlo.
//!
//! Acá no se decide nada: el orden, la numeración, la huella y el modo de ejecución son de
//! `pgforge_core::migrations`. Lo que sí vive acá es lo que no puede vivir en el núcleo: verificar
//! la carpeta contra las rutas elegidas en un diálogo, y lanzar la tarea en segundo plano.

use pgforge_core::migrations::{self, folder, Direction, NewRecord, Overview, Plan};
use pgforge_core::sql::history::{Actor, NewEntry, Source};
use pgforge_core::{Error, ProfileId, Result};
use tauri::{AppHandle, Manager, State};

use crate::commands::{actor_of, tasks};
use crate::process::ProcessKind;
use crate::state::AppState;

fn epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

async fn database_of(state: &AppState, id: ProfileId, database: Option<String>) -> Result<String> {
    match database {
        Some(database) => Ok(database),
        None => Ok(state
            .manager
            .require(id)
            .await?
            .default_database()
            .to_owned()),
    }
}

/// Guarda el script como `V…__descripcion.sql` (y su reversa, si hay) en la carpeta elegida.
///
/// Escribe archivos y nada más: no toca ningún servidor ni anota nada en el registro. Una migración
/// recién guardada está pendiente hasta que se aplique.
#[tauri::command]
pub async fn migration_save(
    state: State<'_, AppState>,
    folder: String,
    description: String,
    up_sql: String,
    down_sql: Option<String>,
) -> Result<folder::Saved> {
    let dir = state.paths.check(&folder)?;
    folder::save(&dir, &description, &up_sql, down_sql.as_deref())
}

/// Qué hay en la carpeta y cómo está cada migración contra este servidor y esta base.
#[tauri::command]
pub async fn migration_overview(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    folder: String,
) -> Result<Overview> {
    let dir = state.paths.check(&folder)?;
    let database = database_of(&state, id, database).await?;
    let records = state
        .migrations
        .lock()
        .await
        .records(&id.to_string(), &database)?;
    migrations::overview(&dir, records)
}

/// Lo que se ejecutaría al aplicar (o deshacer) una migración, leído del archivo en este momento.
#[tauri::command]
pub async fn migration_preview(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    folder: String,
    version: u32,
    direction: Direction,
) -> Result<Plan> {
    let dir = state.paths.check(&folder)?;
    let database = database_of(&state, id, database).await?;
    let records = state
        .migrations
        .lock()
        .await
        .records(&id.to_string(), &database)?;
    migrations::prepare(&dir, &records, version, direction)
}

/// Aplica (o deshace) una migración como tarea en segundo plano y devuelve su identificador.
///
/// `expected_checksum` es la huella que mostró la vista previa: si el archivo cambió entre la vista
/// previa y el botón, no se ejecuta —se aplica lo que se leyó, no lo que hay ahora—. El resultado se
/// anota en el registro **al terminar**, con éxito o con error, y también en el historial como una
/// acción de diálogo, para que aparezca en la auditoría.
// Los argumentos de un comando de Tauri son los campos planos de lo que manda la interfaz.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn migration_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    folder: String,
    version: u32,
    direction: Direction,
    expected_checksum: String,
) -> Result<String> {
    let dir = state.paths.check(&folder)?;
    let database = database_of(&state, id, database).await?;
    let handle = state.manager.require(id).await?;
    handle.profile.ensure_writable("aplicar una migración")?;

    // Dos aplicaciones a la vez sobre la misma base ven el mismo registro, creen que la anterior ya
    // terminó y corren la misma versión dos veces. El cerrojo es de esta base en este servidor.
    let key = (id, database.clone());
    if !state.migrating.lock().await.insert(key.clone()) {
        return Err(Error::Conflict(
            "ya hay una migración corriendo contra esta base: esperá a que termine".to_owned(),
        ));
    }

    let launched = launch(
        app,
        &state,
        id,
        &database,
        &dir,
        version,
        direction,
        &expected_checksum,
    )
    .await;
    if launched.is_err() {
        state.migrating.lock().await.remove(&key);
    }
    launched
}

#[allow(clippy::too_many_arguments)]
async fn launch(
    app: AppHandle,
    state: &AppState,
    id: ProfileId,
    database: &str,
    dir: &std::path::Path,
    version: u32,
    direction: Direction,
    expected_checksum: &str,
) -> Result<String> {
    let records = state
        .migrations
        .lock()
        .await
        .records(&id.to_string(), database)?;
    let plan = migrations::prepare(dir, &records, version, direction)?;
    if plan.checksum != expected_checksum {
        return Err(Error::Conflict(format!(
            "el archivo de V{version:03} cambió desde la vista previa: revisá lo que va a \
             ejecutarse y volvé a confirmar"
        )));
    }
    if plan.execution.steps.is_empty() {
        return Err(Error::Config(
            "el archivo no tiene ninguna sentencia".to_owned(),
        ));
    }

    let actor = actor_of(state, id).await;
    let command = plan.execution.text();
    let target = format!("V{:03} · {}", plan.version, plan.description);

    let done: tasks::Done = {
        let app = app.clone();
        let database = database.to_owned();
        let plan = plan.clone();
        let command = command.clone();
        let actor: Actor = actor.clone();
        Box::new(move |outcome| {
            Box::pin(async move {
                let state = app.state::<AppState>();
                let (seconds, error) = match outcome {
                    Ok(seconds) => (seconds, None),
                    Err(error) => (0.0, Some(error)),
                };
                let at = epoch_seconds();
                // Lo que falle al anotar no toca el resultado: ya corrió contra el servidor.
                let _ = state.migrations.lock().await.record(&NewRecord {
                    profile_id: id.to_string(),
                    database: database.clone(),
                    server_name: actor.server_name.clone(),
                    version: plan.version,
                    description: plan.description.clone(),
                    direction: plan.direction,
                    checksum: plan.checksum.clone(),
                    applied_at: at,
                    seconds,
                    error: error.clone(),
                });
                let _ = state.history.lock().await.record(&NewEntry {
                    source: Source::Dialog,
                    actor,
                    profile_id: id.to_string(),
                    database: database.clone(),
                    sql: command,
                    started_at: at - seconds as i64,
                    seconds,
                    row_count: None,
                    error,
                });
                state.migrating.lock().await.remove(&(id, database));
            })
        })
    };

    tasks::spawn_steps(
        app,
        state,
        ProcessKind::Migration,
        id,
        database.to_owned(),
        target,
        command,
        plan.execution.steps,
        Some(done),
    )
    .await
}
