//! Datos de una tabla.
//!
//! La forma de la tabla viaja de ida y de vuelta en vez de guardarse en el estado: describe el
//! catálogo, no una conexión ni una transacción abierta, y así la interfaz puede mostrar la misma
//! grilla después de un refresco sin que el backend tenga que recordar nada entre llamadas.

use std::path::PathBuf;
use std::time::Instant;

use pgforge_core::data::{
    self, Applied, Change, CopyCommand, Cursor, ExportSpec, ImportSpec, Page, PageView, Statement,
    TableShape,
};
use pgforge_core::error::ErrorPayload;
use pgforge_core::{ProfileId, Result};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, oneshot};

use crate::commands::{record_applied, sql_of};
use crate::process::{Cancel, Outcome, ProcessKind};
use crate::state::AppState;

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

/// La misma forma, pero buscada por nombre.
///
/// Existe porque hay un camino que no empieza en el árbol: la sugerencia de un plan de ejecución
/// dice «esquema.tabla», nunca un oid, y con eso hay que poder abrir el diálogo de índices.
#[tauri::command]
pub async fn data_shape_named(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    schema: String,
    name: String,
) -> Result<TableShape> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::shape_by_name(&handle, &database, &schema, &name).await
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
    view: Option<PageView>,
) -> Result<Page> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::page(
        &handle,
        &database,
        &shape,
        cursor.as_ref(),
        limit.unwrap_or(data::DEFAULT_PAGE_SIZE),
        &view.unwrap_or_default(),
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

    let sql = sql_of(&data::statements(&shape, &changes)?);
    record_applied(
        &state,
        id,
        &database,
        sql,
        data::apply(&handle, &database, &shape, &changes),
    )
    .await
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

/// Lanza la exportación y devuelve su identificador, con el que se la puede cancelar.
#[tauri::command]
pub async fn data_export_run(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    spec: ExportSpec,
    path: String,
    target: String,
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    // Se arma el COPY antes de lanzar la tarea para que una combinación de opciones inválida llegue
    // como el error de este comando y no como un evento de una tarea que ya arrancó.
    let command = data::export_command(&spec)?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut bytes_rx) = mpsc::channel::<u64>(64);
    let path = PathBuf::from(path);

    let task_id = state
        .processes
        .start(
            ProcessKind::Export,
            id,
            database.clone(),
            target,
            command.sql,
            Cancel::Child(cancel),
        )
        .await;

    {
        let task_id = task_id.clone();
        tokio::spawn(async move {
            // El avance se anota a medida que llega: leerlo al final sería una barra falsa durante
            // toda la exportación.
            {
                let app = app.clone();
                let task_id = task_id.clone();
                tokio::spawn(async move {
                    let state = app.state::<AppState>();
                    while let Some(bytes) = bytes_rx.recv().await {
                        state.processes.progress(&task_id, bytes).await;
                    }
                });
            }

            let started = Instant::now();
            let result =
                data::export_to_file(&handle, &database, &spec, &path, progress, cancelled).await;

            let state = app.state::<AppState>();
            match result {
                Ok(outcome) => {
                    state
                        .processes
                        .finish(
                            &task_id,
                            Outcome {
                                seconds: started.elapsed().as_secs_f64(),
                                bytes: Some(outcome.bytes),
                                path: Some(path.display().to_string()),
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
        });
    }

    Ok(task_id)
}

/// El `COPY ... FROM STDIN` que se ejecutaría, sin ejecutar nada.
#[tauri::command]
pub fn data_import_preview(spec: ImportSpec) -> Result<CopyCommand> {
    data::import_command(&spec)
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
) -> Result<String> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let command = data::import_command(&spec)?;

    let (cancel, cancelled) = oneshot::channel();
    let (progress, mut bytes_rx) = mpsc::channel::<u64>(64);
    let path = PathBuf::from(path);

    let task_id = state
        .processes
        .start(
            ProcessKind::Import,
            id,
            database.clone(),
            format!("{}.{}", spec.schema, spec.table),
            command.sql,
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
                    while let Some(bytes) = bytes_rx.recv().await {
                        state.processes.progress(&task_id, bytes).await;
                    }
                });
            }

            let started = Instant::now();
            let result =
                data::import_from_file(&handle, &database, &spec, &path, progress, cancelled).await;

            let state = app.state::<AppState>();
            match result {
                Ok(outcome) => {
                    state
                        .processes
                        .finish(
                            &task_id,
                            Outcome {
                                seconds: started.elapsed().as_secs_f64(),
                                bytes: Some(outcome.bytes),
                                // En importación el núcleo siempre trae las filas; el `unwrap_or(0)`
                                // es solo por completar el tipo, no un caso que pueda darse.
                                rows: Some(outcome.rows.unwrap_or(0)),
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
        });
    }

    Ok(task_id)
}
