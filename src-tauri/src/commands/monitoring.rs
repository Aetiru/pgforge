//! Monitoreo y mantenimiento.
//!
//! El dashboard no consulta: se suscribe. Cada muestra viaja por un canal de Tauri desde una tarea
//! del backend, en vez de que la interfaz llame a un comando cada dos segundos. Así el intervalo lo
//! respeta el que tiene la conexión, y la ventana puede pausarlo cuando deja de estar visible.

use std::sync::Arc;
use std::time::Duration;

use pgforge_core::error::ErrorPayload;
use pgforge_core::monitor::{
    maintenance, ActivityFilter, IndexStat, Lock, Monitor, Operation, Redundancy, Snapshot,
    StatementStat, TableBloat, TableStat, Target,
};
use pgforge_core::{Error, ProfileId, Result};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tokio::sync::Mutex;

use crate::commands::tasks::{self, TaskEvent};
use crate::state::{AppState, MonitorEntry, PollConfig, MIN_POLL_MS};

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MonitorEvent {
    // La muestra va en un `Box` porque es mucho más grande que la variante de error y no tiene
    // sentido que cada evento pague ese tamaño.
    Snapshot { snapshot: Box<Snapshot> },
    Error { error: ErrorPayload },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorOptions {
    pub interval_ms: Option<u64>,
    pub paused: Option<bool>,
    pub filter: Option<ActivityFilter>,
}

impl MonitorOptions {
    fn apply(self, config: &mut PollConfig) {
        if let Some(interval) = self.interval_ms {
            config.interval_ms = interval.max(MIN_POLL_MS);
        }
        if let Some(paused) = self.paused {
            config.paused = paused;
        }
        if let Some(filter) = self.filter {
            config.filter = filter;
        }
    }
}

/// Arranca el monitoreo del servidor. Volver a llamarlo reemplaza la suscripción anterior.
#[tauri::command]
pub async fn monitor_start(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    options: Option<MonitorOptions>,
    channel: Channel<MonitorEvent>,
) -> Result<()> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let monitor = Arc::new(Mutex::new(Monitor::open(&handle, &database).await?));

    let mut config = PollConfig::default();
    if let Some(options) = options {
        options.apply(&mut config);
    }
    let config = Arc::new(Mutex::new(config));

    let task = tokio::spawn(poll_loop(monitor.clone(), config.clone(), channel));

    // Insertar descarta la entrada anterior, y su `Drop` aborta la tarea que tenía andando.
    state.monitors.lock().await.insert(
        id,
        MonitorEntry {
            monitor,
            config,
            task,
        },
    );

    Ok(())
}

async fn poll_loop(
    monitor: Arc<Mutex<Monitor>>,
    config: Arc<Mutex<PollConfig>>,
    channel: Channel<MonitorEvent>,
) {
    loop {
        let (interval, paused, filter) = {
            let config = config.lock().await;
            (
                config.interval_ms.max(MIN_POLL_MS),
                config.paused,
                config.filter.clone(),
            )
        };

        if !paused {
            let result = monitor.lock().await.snapshot(&filter).await;
            let event = match result {
                Ok(snapshot) => MonitorEvent::Snapshot {
                    snapshot: Box::new(snapshot),
                },
                // Un error puntual —el servidor reinició, se cayó la red— no debe matar la
                // suscripción: se informa y se vuelve a intentar en el próximo ciclo.
                Err(error) => MonitorEvent::Error {
                    error: ErrorPayload::from(&error),
                },
            };

            // El canal cerrado significa que del otro lado ya no hay nadie escuchando.
            if channel.send(event).is_err() {
                break;
            }
        }

        tokio::time::sleep(Duration::from_millis(interval)).await;
    }
}

#[tauri::command]
pub async fn monitor_stop(state: State<'_, AppState>, id: ProfileId) -> Result<()> {
    state.monitors.lock().await.remove(&id);
    Ok(())
}

/// Cambia intervalo, pausa o filtro sin reabrir la conexión de monitoreo.
#[tauri::command]
pub async fn monitor_configure(
    state: State<'_, AppState>,
    id: ProfileId,
    options: MonitorOptions,
) -> Result<()> {
    let monitors = state.monitors.lock().await;
    let entry = monitors
        .get(&id)
        .ok_or_else(|| Error::Config("el monitoreo no está activo".to_owned()))?;

    options.apply(&mut *entry.config.lock().await);
    Ok(())
}

async fn monitor_of(state: &AppState, id: ProfileId) -> Result<Arc<Mutex<Monitor>>> {
    state
        .monitors
        .lock()
        .await
        .get(&id)
        .map(|entry| entry.monitor.clone())
        .ok_or_else(|| Error::Config("el monitoreo no está activo para este servidor".to_owned()))
}

/// Muestra inmediata, sin esperar al próximo ciclo.
#[tauri::command]
pub async fn monitor_refresh(
    state: State<'_, AppState>,
    id: ProfileId,
    filter: Option<ActivityFilter>,
) -> Result<Snapshot> {
    let monitor = monitor_of(&state, id).await?;
    let filter = filter.unwrap_or_default();
    let snapshot = monitor.lock().await.snapshot(&filter).await?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn backend_locks(
    state: State<'_, AppState>,
    id: ProfileId,
    pid: i32,
) -> Result<Vec<Lock>> {
    let monitor = monitor_of(&state, id).await?;
    let locks = monitor.lock().await.locks(pid).await?;
    Ok(locks)
}

/// Aborta la consulta en curso de una sesión, dejándola conectada.
#[tauri::command]
pub async fn cancel_backend(state: State<'_, AppState>, id: ProfileId, pid: i32) -> Result<bool> {
    let monitor = monitor_of(&state, id).await?;
    let result = monitor.lock().await.cancel_backend(pid).await?;
    Ok(result)
}

/// Cierra la sesión entera: revierte su transacción y desconecta al cliente.
#[tauri::command]
pub async fn terminate_backend(
    state: State<'_, AppState>,
    id: ProfileId,
    pid: i32,
) -> Result<bool> {
    let monitor = monitor_of(&state, id).await?;
    let result = monitor.lock().await.terminate_backend(pid).await?;
    Ok(result)
}

#[tauri::command]
pub async fn table_stats(
    state: State<'_, AppState>,
    id: ProfileId,
    limit: Option<i64>,
) -> Result<Vec<TableStat>> {
    let monitor = monitor_of(&state, id).await?;
    let stats = monitor.lock().await.tables(limit.unwrap_or(200)).await?;
    Ok(stats)
}

#[tauri::command]
pub async fn index_stats(
    state: State<'_, AppState>,
    id: ProfileId,
    limit: Option<i64>,
) -> Result<Vec<IndexStat>> {
    let monitor = monitor_of(&state, id).await?;
    let stats = monitor.lock().await.indexes(limit.unwrap_or(200)).await?;
    Ok(stats)
}

/// Los índices que sobran porque otro los cubre. Solo lee: borrarlos es una decisión aparte.
#[tauri::command]
pub async fn redundant_indexes(
    state: State<'_, AppState>,
    id: ProfileId,
) -> Result<Vec<Redundancy>> {
    let monitor = monitor_of(&state, id).await?;
    let sobran = monitor.lock().await.redundant_indexes().await?;
    Ok(sobran)
}

#[tauri::command]
pub async fn has_statement_stats(state: State<'_, AppState>, id: ProfileId) -> Result<bool> {
    let monitor = monitor_of(&state, id).await?;
    let available = monitor.lock().await.has_statement_stats().await?;
    Ok(available)
}

#[tauri::command]
pub async fn has_bloat_stats(state: State<'_, AppState>, id: ProfileId) -> Result<bool> {
    let monitor = monitor_of(&state, id).await?;
    let available = monitor.lock().await.has_bloat_stats().await?;
    Ok(available)
}

#[tauri::command]
pub async fn table_bloat(
    state: State<'_, AppState>,
    id: ProfileId,
    limit: Option<i64>,
) -> Result<Vec<TableBloat>> {
    let monitor = monitor_of(&state, id).await?;
    let bloat = monitor.lock().await.bloat(limit.unwrap_or(50)).await?;
    Ok(bloat)
}

#[tauri::command]
pub async fn statement_stats(
    state: State<'_, AppState>,
    id: ProfileId,
    limit: Option<i64>,
) -> Result<Vec<StatementStat>> {
    let monitor = monitor_of(&state, id).await?;
    let stats = monitor.lock().await.statements(limit.unwrap_or(50)).await?;
    Ok(stats)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenancePlan {
    sql: String,
    /// Advertencia que hay que mostrar antes de ejecutar, si la operación bloquea la tabla.
    warning: Option<String>,
}

/// Devuelve la sentencia que se va a ejecutar, para mostrarla antes de lanzarla.
#[tauri::command]
pub async fn maintenance_plan(
    state: State<'_, AppState>,
    id: ProfileId,
    operation: Operation,
    target: Target,
) -> Result<MaintenancePlan> {
    let handle = state.manager.require(id).await?;
    Ok(MaintenancePlan {
        sql: maintenance::statement(operation, &target, &handle.caps)?,
        warning: maintenance::warning(operation).map(str::to_owned),
    })
}

/// Lanza la tarea y devuelve su identificador, con el que se la puede cancelar (`task_cancel`).
///
/// Correrla y seguirla son dos cosas distintas: acá solo se arma la sentencia y se la larga, y de
/// esperarla se encarga `tasks::spawn_statement`, el mismo que usa la creación de un índice.
#[tauri::command]
pub async fn maintenance_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    operation: Operation,
    target: Target,
    channel: Channel<TaskEvent>,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());
    let sql = maintenance::statement(operation, &target, &handle.caps)?;

    tasks::spawn_statement(app, &state, id, database, sql, channel).await
}
