use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use pgforge_core::monitor::{ActivityFilter, Monitor};
use pgforge_core::sql::{HistoryStore, QuerySession};
use pgforge_core::{ConnectionManager, ProfileId, ProfileStore};
use tauri::ipc::Channel;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_postgres::CancelToken;

use crate::commands::query::QueryEvent;

/// Refresco por omisión del dashboard.
pub const DEFAULT_POLL_MS: u64 = 2_000;

/// Piso del intervalo. Por debajo de esto el monitoreo pesa más que lo que mide.
pub const MIN_POLL_MS: u64 = 250;

/// Lo que el dashboard puede cambiar sin reiniciar el monitoreo.
#[derive(Debug, Clone)]
pub struct PollConfig {
    pub interval_ms: u64,
    /// Se pone en `true` cuando la ventana deja de estar visible. Un refresco cada dos segundos
    /// contra un servidor de producción, con la ventana minimizada, es trabajo puro sin nadie que
    /// lo mire.
    pub paused: bool,
    pub filter: ActivityFilter,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            interval_ms: DEFAULT_POLL_MS,
            paused: false,
            filter: ActivityFilter::default(),
        }
    }
}

pub struct MonitorEntry {
    pub monitor: Arc<Mutex<Monitor>>,
    pub config: Arc<Mutex<PollConfig>>,
    pub task: JoinHandle<()>,
}

impl Drop for MonitorEntry {
    fn drop(&mut self) {
        // Sin esto, el bucle de sondeo sigue consultando un servidor que ya nadie está mirando.
        self.task.abort();
    }
}

/// Una tarea de mantenimiento en curso, con la vía para abortarla.
///
/// No se guarda el `JoinHandle`: cancelar una tarea de mantenimiento se hace pidiéndoselo al
/// servidor, no matando la tarea local. Abortarla del lado de la aplicación dejaría al servidor
/// terminando el `VACUUM` igual, pero sin nadie escuchando su resultado.
pub struct MaintenanceEntry {
    /// Servidor sobre el que corre, necesario para abrir la conexión de cancelación con el mismo
    /// cifrado que el resto.
    pub profile: ProfileId,
    pub cancel: CancelToken,
}

/// Un backup en curso.
///
/// A diferencia de [`MaintenanceEntry`], acá el trabajo lo hace un proceso hijo de la aplicación y
/// no el servidor: lo que se guarda es el extremo por el que se le avisa que lo mate, y el núcleo
/// se encarga de borrar el archivo a medio escribir.
pub struct BackupEntry {
    pub cancel: oneshot::Sender<()>,
}

/// Una pestaña de consulta abierta, con su conexión propia.
///
/// La sesión vive acá y no dentro de la llamada que ejecuta: es lo que hace que un `BEGIN`, un
/// `SET` o una tabla temporal sigan valiendo en la consulta siguiente de la misma pestaña.
pub struct QueryEntry {
    pub profile: ProfileId,
    /// Se guarda para el historial: la pestaña queda atada a la base con la que se abrió.
    pub database: String,
    pub session: Arc<Mutex<QuerySession>>,
    pub cancel: CancelToken,
    /// Canal de la ejecución en curso. Los `RAISE NOTICE` llegan por una tarea aparte que vive
    /// mientras dura la pestaña, así que necesita saber a dónde mandarlos en cada momento.
    pub notices: Arc<Mutex<Option<Channel<QueryEvent>>>>,
}

pub struct AppState {
    pub manager: ConnectionManager,
    pub store: Mutex<ProfileStore>,
    pub monitors: Mutex<HashMap<ProfileId, MonitorEntry>>,
    pub maintenance: Mutex<HashMap<String, MaintenanceEntry>>,
    pub backups: Mutex<HashMap<String, BackupEntry>>,
    pub queries: Mutex<HashMap<String, QueryEntry>>,
    pub history: Mutex<HistoryStore>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> pgforge_core::Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        Ok(Self {
            manager: ConnectionManager::new(),
            store: Mutex::new(ProfileStore::load(config_dir.join("connections.json"))?),
            history: Mutex::new(HistoryStore::open(config_dir.join("history.db"))?),
            monitors: Mutex::new(HashMap::new()),
            maintenance: Mutex::new(HashMap::new()),
            backups: Mutex::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
        })
    }
}
