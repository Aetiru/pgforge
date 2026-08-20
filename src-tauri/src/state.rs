use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use pgforge_core::conn::CancelSink;
use pgforge_core::monitor::{ActivityFilter, Monitor};
use pgforge_core::sql::{HistoryStore, QuerySession, SavedStore};
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

/// Una sentencia larga en curso del lado del servidor, con la vía para abortarla.
///
/// Son las que corren solas mientras se sigue usando la aplicación: el mantenimiento y la creación
/// de un índice. No se guarda el `JoinHandle`, porque cancelarlas se hace pidiéndoselo al servidor y
/// no matando la tarea local: abortarla del lado de la aplicación dejaría al servidor terminando el
/// `VACUUM` igual, pero sin nadie escuchando su resultado.
pub struct TaskEntry {
    /// Servidor sobre el que corre, necesario para abrir la conexión de cancelación con el mismo
    /// cifrado que el resto.
    pub profile: ProfileId,
    pub cancel: CancelToken,
}

/// Un proceso externo en curso: un backup o un restore.
///
/// A diferencia de [`TaskEntry`], acá el trabajo lo hace un proceso hijo de la aplicación y
/// no el servidor: lo que se guarda es el extremo por el que se le avisa que lo mate. Qué limpiar
/// tras cancelarlo lo resuelve el núcleo —el backup borra su archivo a medio escribir, el restore
/// no tiene nada que borrar del disco—.
pub struct ExternalTask {
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
    /// Sentencias largas en curso —mantenimiento, creación de índices—, por identificador.
    pub tasks: Mutex<HashMap<String, TaskEntry>>,
    pub backups: Mutex<HashMap<String, ExternalTask>>,
    pub restores: Mutex<HashMap<String, ExternalTask>>,
    /// Exportaciones e importaciones de datos en curso. Comparten el mismo mapa: cada una tiene su
    /// identificador único y ambas se cancelan igual, avisándole a la tarea que corte.
    pub copies: Mutex<HashMap<String, ExternalTask>>,
    pub queries: Mutex<HashMap<String, QueryEntry>>,
    /// Lecturas del árbol y del DDL en curso, por identificador de pedido.
    pub reads: Mutex<HashMap<String, ReadEntry>>,
    pub history: Mutex<HistoryStore>,
    /// Las consultas que el usuario decidió conservar. Archivo aparte del historial: son cosas
    /// distintas y el `user_version` del esquema es del archivo (ver `sql::saved`).
    pub saved: Mutex<SavedStore>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> pgforge_core::Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        Ok(Self {
            manager: ConnectionManager::new(),
            store: Mutex::new(ProfileStore::load(config_dir.join("connections.json"))?),
            history: Mutex::new(HistoryStore::open(config_dir.join("history.db"))?),
            saved: Mutex::new(SavedStore::open(config_dir.join("saved.db"))?),
            monitors: Mutex::new(HashMap::new()),
            tasks: Mutex::new(HashMap::new()),
            backups: Mutex::new(HashMap::new()),
            restores: Mutex::new(HashMap::new()),
            copies: Mutex::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
            reads: Mutex::new(HashMap::new()),
        })
    }
}
