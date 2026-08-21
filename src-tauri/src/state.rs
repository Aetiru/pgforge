use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use pgforge_core::conn::CancelSink;
use pgforge_core::monitor::{ActivityFilter, Monitor};
use pgforge_core::sql::{HistoryStore, QuerySession, SavedStore, SnippetStore};
use pgforge_core::{ConnectionManager, ProfileId, ProfileStore};
use tauri::ipc::Channel;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_postgres::CancelToken;

use crate::commands::query::QueryEvent;
use crate::process::Processes;

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
    /// Si cada ejecución se confirma sola. Arranca con el valor del perfil y la pestaña lo alterna.
    /// Vive del lado de Rust y no en la interfaz porque es acá donde se decide si va un `BEGIN`.
    pub autocommit: Arc<AtomicBool>,
    /// Canal de la ejecución en curso. Los `RAISE NOTICE` llegan por una tarea aparte que vive
    /// mientras dura la pestaña, así que necesita saber a dónde mandarlos en cada momento.
    pub notices: Arc<Mutex<Option<Channel<QueryEvent>>>>,
}

/// Una lectura en curso que se puede abortar: la carga de un nodo del árbol o el DDL de un objeto.
///
/// No tienen sesión propia como la pestaña de consulta —toman una conexión del pool y la devuelven—,
/// así que lo que se guarda es el `CancelSink` donde el núcleo anota los tokens de las conexiones
/// que la lectura fue usando.
pub struct ReadEntry {
    pub profile: ProfileId,
    pub sink: CancelSink,
}

pub struct AppState {
    pub manager: ConnectionManager,
    pub store: Mutex<ProfileStore>,
    pub monitors: Mutex<HashMap<ProfileId, MonitorEntry>>,
    /// Todo lo que corre en segundo plano: mantenimiento, índices, backups, restores y copias de
    /// datos. Vive acá y no en la ventana porque el proceso de Rust le sobrevive a una recarga (ver
    /// [`crate::process`]).
    pub processes: Processes,
    pub queries: Mutex<HashMap<String, QueryEntry>>,
    /// Lecturas del árbol y del DDL en curso, por identificador de pedido.
    pub reads: Mutex<HashMap<String, ReadEntry>>,
    pub history: Mutex<HistoryStore>,
    /// Las consultas que el usuario decidió conservar. Archivo aparte del historial: son cosas
    /// distintas y el `user_version` del esquema es del archivo (ver `sql::saved`).
    pub saved: Mutex<SavedStore>,
    /// Las abreviaturas del editor. Archivo JSON y no SQLite como las guardadas: es una lista corta
    /// que se edita entera a mano, y poder abrirla con un editor de texto es parte de la gracia.
    pub snippets: Mutex<SnippetStore>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> pgforge_core::Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        Ok(Self {
            manager: ConnectionManager::new(),
            store: Mutex::new(ProfileStore::load(config_dir.join("connections.json"))?),
            history: Mutex::new(HistoryStore::open(config_dir.join("history.db"))?),
            saved: Mutex::new(SavedStore::open(config_dir.join("saved.db"))?),
            snippets: Mutex::new(SnippetStore::load(config_dir.join("snippets.json"))?),
            monitors: Mutex::new(HashMap::new()),
            processes: Processes::default(),
            queries: Mutex::new(HashMap::new()),
            reads: Mutex::new(HashMap::new()),
        })
    }
}
