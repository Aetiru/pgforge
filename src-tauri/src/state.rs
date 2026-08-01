use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use pgforge_core::monitor::{ActivityFilter, Monitor};
use pgforge_core::{ConnectionManager, ProfileId, ProfileStore};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_postgres::CancelToken;

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

pub struct AppState {
    pub manager: ConnectionManager,
    pub store: Mutex<ProfileStore>,
    pub monitors: Mutex<HashMap<ProfileId, MonitorEntry>>,
    pub maintenance: Mutex<HashMap<String, MaintenanceEntry>>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> pgforge_core::Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        Ok(Self {
            manager: ConnectionManager::new(),
            store: Mutex::new(ProfileStore::load(config_dir.join("connections.json"))?),
            monitors: Mutex::new(HashMap::new()),
            maintenance: Mutex::new(HashMap::new()),
        })
    }
}
