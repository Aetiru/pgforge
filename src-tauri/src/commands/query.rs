//! Ejecución de SQL escrito por el usuario.
//!
//! Cada pestaña del editor abre su propia conexión y la conserva mientras siga abierta. Los
//! resultados no se devuelven de una: viajan por un canal a medida que cada sentencia termina, con
//! el mismo mecanismo que usa el mantenimiento. Un script de diez sentencias tiene que mostrar la
//! primera cuando termina la primera, no cuando terminan las diez.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pgforge_core::error::ErrorPayload;
use pgforge_core::sql::{
    self, completion, explain, history::Entry as HistoryEntry, history::Source as HistorySource,
    ColumnType, ExplainOptions, Limits, NewEntry, NewQuery, Outcome, Plan, QuerySession,
    SavedQuery, SchemaSnapshot, TxStatus,
};
use pgforge_core::{Error, ProfileId, Result};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;
use tokio::sync::Mutex;

use crate::state::{AppState, QueryEntry};

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum QueryEvent {
    /// Empezó una sentencia del script.
    #[serde(rename_all = "camelCase")]
    Started {
        index: usize,
        total: usize,
        line: usize,
    },
    /// La sentencia devolvió filas. Va en `Box` porque es mucho más grande que el resto y no tiene
    /// sentido que cada evento pague ese tamaño.
    #[serde(rename_all = "camelCase")]
    Finished {
        index: usize,
        outcome: Box<Outcome>,
    },
    Notice {
        severity: String,
        message: String,
    },
    /// Falló una sentencia; las que quedaban no se ejecutan.
    #[serde(rename_all = "camelCase")]
    Failed {
        index: usize,
        error: ErrorPayload,
        /// Dónde empieza la sentencia dentro del script, en caracteres. Sumado a la posición que
        /// trae el error, ubica el carácter exacto en el editor.
        offset: usize,
    },
    /// Cómo quedó la transacción de la pestaña. Se emite al terminar el script, haya fallado o no:
    /// una sentencia rota dentro de una transacción la deja abortada, y eso hay que mostrarlo.
    #[serde(rename_all = "camelCase")]
    Transaction {
        status: TxStatus,
    },
    /// Terminó el script entero.
    #[serde(rename_all = "camelCase")]
    Completed {
        seconds: f64,
        /// Cuántas sentencias llegaron a ejecutarse.
        executed: usize,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryTab {
    pub tab_id: String,
    /// La base que quedó abierta, que puede ser la del perfil si no se pidió otra.
    pub database: String,
    /// Valor heredado del perfil; la pestaña lo puede cambiar con `query_autocommit`.
    pub autocommit: bool,
    pub tx_status: TxStatus,
}

/// Abre la conexión de una pestaña de consulta.
#[tauri::command]
pub async fn query_open(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
) -> Result<QueryTab> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    let mut session = QuerySession::open(&handle, &database).await?;
    let cancel = session.cancel_token();
    let notices = session.take_notices();
    let sink = Arc::new(Mutex::new(None));
    let tab_id = uuid::Uuid::new_v4().to_string();

    // Los NOTICE llegan por la conexión en cualquier momento, no como respuesta a una consulta:
    // una tarea de por vida los reparte a la ejecución que esté en curso, si es que hay alguna.
    if let Some(mut notices) = notices {
        let sink: Arc<Mutex<Option<Channel<QueryEvent>>>> = sink.clone();
        tokio::spawn(async move {
            while let Some(notice) = notices.recv().await {
                if let Some(channel) = sink.lock().await.as_ref() {
                    let _ = channel.send(QueryEvent::Notice {
                        severity: notice.severity,
                        message: notice.message,
                    });
                }
            }
        });
    }

    let autocommit = handle.profile.autocommit;

    state.queries.lock().await.insert(
        tab_id.clone(),
        QueryEntry {
            profile: id,
            database: database.clone(),
            session: Arc::new(Mutex::new(session)),
            cancel,
            notices: sink,
            autocommit: Arc::new(AtomicBool::new(autocommit)),
        },
    );

    Ok(QueryTab {
        tab_id,
        database,
        autocommit,
        // Una sesión recién abierta no tiene nada empezado; no hace falta preguntárselo al servidor.
        tx_status: TxStatus::Idle,
    })
}

/// Cierra la pestaña y suelta su conexión.
#[tauri::command]
pub async fn query_close(state: State<'_, AppState>, tab_id: String) -> Result<()> {
    state.queries.lock().await.remove(&tab_id);
    Ok(())
}

/// Ejecuta un script sentencia por sentencia.
///
/// Los errores viajan por el canal y no como falla de la llamada: la interfaz necesita saber
/// *cuál* de las sentencias falló y dónde, y eso no entra en un `Err` genérico.
#[tauri::command]
pub async fn query_run(
    state: State<'_, AppState>,
    tab_id: String,
    sql: String,
    limits: Option<Limits>,
    channel: Channel<QueryEvent>,
) -> Result<()> {
    let (profile, database, session, sink, autocommit) = {
        let tabs = state.queries.lock().await;
        let entry = require(&tabs, &tab_id)?;
        (
            entry.profile,
            entry.database.clone(),
            entry.session.clone(),
            entry.notices.clone(),
            entry.autocommit.load(Ordering::Relaxed),
        )
    };

    let statements = sql::split(&sql);
    if statements.is_empty() {
        return Err(Error::Config(
            "no hay ninguna sentencia para ejecutar".to_owned(),
        ));
    }

    let limits = limits.unwrap_or_default();
    let started = Instant::now();
    let started_at = epoch_seconds();
    let mut executed = 0;
    let mut rows_returned = None;
    let mut failure = None;

    // La sesión queda tomada durante todo el script: es una sola conexión, y dejar que dos
    // ejecuciones se intercalen mezclaría las transacciones de la pestaña.
    let session = session.lock().await;
    *sink.lock().await = Some(channel.clone());

    // Con autocommit apagado, la transacción la abre pgforge antes de la primera sentencia: el
    // servidor no tiene modo autocommit que apagar. Si ya había una abierta, esto no hace nada.
    if !autocommit {
        if let Err(error) = session.begin_if_needed().await {
            *sink.lock().await = None;
            return Err(error);
        }
    }

    for (index, statement) in statements.iter().enumerate() {
        let _ = channel.send(QueryEvent::Started {
            index,
            total: statements.len(),
            line: statement.line,
        });

        match session.run(&statement.text, limits).await {
            Ok(outcome) => {
                executed += 1;
                if let Outcome::Rows { row_count, .. } = &outcome {
                    rows_returned = Some(*row_count as i64);
                }
                let _ = channel.send(QueryEvent::Finished {
                    index,
                    outcome: Box::new(outcome),
                });
            }
            Err(error) => {
                let _ = channel.send(QueryEvent::Failed {
                    index,
                    error: ErrorPayload::from(&error),
                    offset: statement.offset,
                });
                failure = Some(error.to_string());
                break;
            }
        }
    }

    // Cómo quedó la transacción se pregunta acá y no se deduce del SQL: el usuario pudo escribir su
    // propio `BEGIN`, un `COMMIT` o dejarla abortada, y la interfaz tiene que mostrar lo que hay.
    // Que falle la sonda no invalida lo que ya se ejecutó, así que solo se omite el evento.
    if let Ok(status) = session.tx_status().await {
        let _ = channel.send(QueryEvent::Transaction { status });
    }

    *sink.lock().await = None;
    let seconds = started.elapsed().as_secs_f64();

    // Se registra lo que el usuario mandó a ejecutar, no cada sentencia por separado: es lo que va
    // a querer recuperar del historial, y es lo que reconoce cuando lo ve.
    let recorded = state.history.lock().await.record(&NewEntry {
        source: HistorySource::Editor,
        profile_id: profile.to_string(),
        database,
        sql,
        started_at,
        seconds,
        row_count: rows_returned,
        error: failure,
    });
    // Que falle el historial no puede tirar abajo una consulta que ya se ejecutó: se avisa junto a
    // los demás mensajes y la ejecución se da por buena igual.
    if let Err(error) = recorded {
        let _ = channel.send(QueryEvent::Notice {
            severity: "WARNING".to_owned(),
            message: error.to_string(),
        });
    }

    let _ = channel.send(QueryEvent::Completed { seconds, executed });

    Ok(())
}

/// Segundos desde el epoch. Un reloj corrido hacia atrás daría un instante negativo; se recorta a
/// cero antes que romper el registro por algo que no importa.
fn epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// Últimas consultas ejecutadas, de un servidor o de todos.
#[tauri::command]
pub async fn history_recent(
    state: State<'_, AppState>,
    id: Option<ProfileId>,
    limit: Option<i64>,
) -> Result<Vec<HistoryEntry>> {
    let profile = id.map(|id| id.to_string());
    state
        .history
        .lock()
        .await
        .recent(profile.as_deref(), limit.unwrap_or(100))
}

#[tauri::command]
pub async fn history_search(
    state: State<'_, AppState>,
    text: String,
    limit: Option<i64>,
) -> Result<Vec<HistoryEntry>> {
    state
        .history
        .lock()
        .await
        .search(&text, limit.unwrap_or(100))
}

#[tauri::command]
pub async fn history_clear(state: State<'_, AppState>) -> Result<()> {
    state.history.lock().await.clear()
}

/// Las consultas guardadas, por nombre.
#[tauri::command]
pub async fn saved_list(state: State<'_, AppState>) -> Result<Vec<SavedQuery>> {
    state.saved.lock().await.list()
}

/// Guarda una consulta nueva o reescribe una existente. El reloj lo pone acá quien tiene el sistema
/// operativo, no el núcleo, igual que en el historial.
#[tauri::command]
pub async fn saved_save(state: State<'_, AppState>, query: NewQuery) -> Result<SavedQuery> {
    state.saved.lock().await.save(&query, epoch_seconds())
}

#[tauri::command]
pub async fn saved_delete(state: State<'_, AppState>, saved_id: i64) -> Result<bool> {
    state.saved.lock().await.delete(saved_id)
}

/// Pide al servidor que aborte lo que la pestaña esté ejecutando.
#[tauri::command]
pub async fn query_cancel(state: State<'_, AppState>, tab_id: String) -> Result<()> {
    let (profile, cancel) = {
        let tabs = state.queries.lock().await;
        let entry = require(&tabs, &tab_id)?;
        (entry.profile, entry.cancel.clone())
    };

    // Se le pide al servidor, no se aborta la tarea local: matarla del lado de la aplicación
    // dejaría al servidor ejecutando la consulta igual, sin nadie esperando el resultado.
    let handle = state.manager.require(profile).await?;
    handle.cancel(&cancel).await
}

/// Confirma la transacción de la pestaña y devuelve cómo quedó.
#[tauri::command]
pub async fn query_commit(state: State<'_, AppState>, tab_id: String) -> Result<TxStatus> {
    let session = session_of(&state, &tab_id).await?;
    let session = session.lock().await;

    // El estado se consulta aunque el `COMMIT` falle —una restricción diferida puede voltearlo—,
    // porque la interfaz tiene que mostrar la transacción real, no la que se esperaba.
    let applied = session.commit().await;
    let status = session.tx_status().await;
    applied?;
    status
}

/// Revierte la transacción de la pestaña, esté sana o abortada.
#[tauri::command]
pub async fn query_rollback(state: State<'_, AppState>, tab_id: String) -> Result<TxStatus> {
    let session = session_of(&state, &tab_id).await?;
    let session = session.lock().await;

    let applied = session.rollback().await;
    let status = session.tx_status().await;
    applied?;
    status
}

/// Los tipos de las columnas que devolvería la sentencia, sin ejecutarla.
///
/// Se pide aparte porque cuesta preparar la sentencia en el servidor: la interfaz lo hace solo
/// cuando el usuario pide ver los tipos.
#[tauri::command]
pub async fn query_column_types(
    state: State<'_, AppState>,
    tab_id: String,
    sql: String,
) -> Result<Vec<ColumnType>> {
    let session = session_of(&state, &tab_id).await?;
    let session = session.lock().await;
    session.column_types(&sql).await
}

/// Estado de la transacción de la pestaña, para volver a sincronizar la interfaz.
#[tauri::command]
pub async fn query_tx_status(state: State<'_, AppState>, tab_id: String) -> Result<TxStatus> {
    let session = session_of(&state, &tab_id).await?;
    let session = session.lock().await;
    session.tx_status().await
}

/// Enciende o apaga el autocommit de la pestaña.
///
/// Encenderlo **no confirma nada**: solo deja de anteponer el `BEGIN`. Si quedaba una transacción
/// abierta se devuelve tal cual está, para que la interfaz pida confirmarla o revertirla — commitear
/// en silencio por tocar un interruptor es exactamente lo que estos botones existen para evitar.
#[tauri::command]
pub async fn query_autocommit(
    state: State<'_, AppState>,
    tab_id: String,
    enabled: bool,
) -> Result<TxStatus> {
    let (session, autocommit) = {
        let tabs = state.queries.lock().await;
        let entry = require(&tabs, &tab_id)?;
        (entry.session.clone(), entry.autocommit.clone())
    };

    autocommit.store(enabled, Ordering::Relaxed);
    let session = session.lock().await;
    session.tx_status().await
}

/// Sesión de una pestaña, para los comandos que solo necesitan eso.
async fn session_of(state: &State<'_, AppState>, tab_id: &str) -> Result<Arc<Mutex<QuerySession>>> {
    let tabs = state.queries.lock().await;
    Ok(require(&tabs, tab_id)?.session.clone())
}

/// Plan de ejecución de una sentencia.
#[tauri::command]
pub async fn query_explain(
    state: State<'_, AppState>,
    tab_id: String,
    sql: String,
    options: Option<ExplainOptions>,
) -> Result<Plan> {
    let session = {
        let tabs = state.queries.lock().await;
        require(&tabs, &tab_id)?.session.clone()
    };

    let options = options.unwrap_or_default();
    let session = session.lock().await;

    // Del script se explica una sola sentencia: un plan por sentencia no es un plan.
    let statements = sql::split(&sql);
    let statement = statements
        .first()
        .ok_or_else(|| Error::Config("no hay ninguna sentencia para explicar".to_owned()))?;

    explain::explain(&session, &statement.text, options).await
}

/// Nombres del esquema para el autocompletado. La interfaz lo pide una vez por pestaña.
#[tauri::command]
pub async fn schema_snapshot(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
) -> Result<SchemaSnapshot> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    completion::snapshot(&handle, &database).await
}

/// La sentencia sobre la que está el cursor, para poder ejecutar solo esa.
///
/// Lo resuelve el núcleo y no la interfaz: reimplementar en TypeScript las reglas de dónde termina
/// una sentencia daría dos partidores que se van a desincronizar el día que aparezca un caso raro.
#[tauri::command]
pub fn statement_at_cursor(sql: String, cursor: usize) -> Option<sql::Statement> {
    sql::at_cursor(&sql, cursor)
}

/// Guarda el texto de una pestaña de consulta en la ruta elegida.
///
/// Misma excepción anotada que `erd_export_svg`: lo que se escribe lo tiene la interfaz y no el
/// servidor, así que no hay lógica que llevar al núcleo, y estas dos líneas salen más baratas que
/// sumar el complemento de archivos con su ámbito de permisos.
#[tauri::command]
pub fn sql_write_file(path: String, sql: String) -> Result<()> {
    std::fs::write(path, sql)?;
    Ok(())
}

/// Lee un archivo de SQL para abrirlo en una pestaña. Misma excepción que el de arriba.
///
/// El texto se lee con pérdida (`from_utf8_lossy`) en vez de fallar: un script viejo guardado en
/// Latin-1 tiene que poder abrirse y arreglarse, y negarse a mostrarlo por un acento roto no ayuda
/// a nadie.
#[tauri::command]
pub fn sql_read_file(path: String) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Advertencia previa a un `EXPLAIN ANALYZE`, que ejecuta la sentencia de verdad.
///
/// Es una consulta al núcleo y no una regla escrita en la interfaz para que la CLI avise lo mismo.
#[tauri::command]
pub fn explain_warning(sql: String, options: Option<ExplainOptions>) -> Option<String> {
    explain::warning(&sql, options.unwrap_or_default()).map(str::to_owned)
}

fn require<'a>(
    tabs: &'a std::collections::HashMap<String, QueryEntry>,
    tab_id: &str,
) -> Result<&'a QueryEntry> {
    tabs.get(tab_id)
        .ok_or_else(|| Error::Config("la pestaña de consulta ya no está abierta".to_owned()))
}
