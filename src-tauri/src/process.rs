//! El registro de los procesos largos, que es donde vive lo que corre en segundo plano.
//!
//! Antes cada proceso tenía su canal propio hacia la interfaz y del lado de Rust solo quedaba lo
//! mínimo para cancelarlo: qué había impreso, cuánto llevaba y con qué terminó vivía únicamente en
//! la ventana. Eso volvía la ventana la única dueña de esa información, y una recarga —`F5` en
//! desarrollo, un reinicio del webview— la borraba entera mientras el `VACUUM` seguía corriendo del
//! otro lado. Peor todavía: si el backup terminaba justo durante la recarga, su resultado no
//! quedaba en ningún lado y nadie se enteraba de si había salido bien.
//!
//! Acá el dueño es el proceso de Rust, que sobrevive a la ventana, y la interfaz es un espejo: se
//! engancha con un solo canal, recibe de entrada todo lo que hay y después las novedades. Cancelar
//! sigue siendo explícito y sigue siendo lo mismo de siempre —al servidor se le pide que aborte, al
//! proceso hijo se le avisa que corte—, solo que ahora las dos vías viven en el mismo registro y
//! hay un único comando para cancelar en vez de uno por clase de proceso.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use pgforge_core::error::ErrorPayload;
use pgforge_core::ProfileId;
use serde::Serialize;
use tauri::ipc::Channel;
use tokio::sync::{oneshot, Mutex};
use tokio_postgres::CancelToken;

/// Cuántas líneas de salida se conservan por proceso.
///
/// `pg_restore` sobre una base grande imprime miles: guardarlas todas sería tener el volcado entero
/// en memoria para mirar el final. Se conservan las últimas porque es donde están los errores que
/// explican cómo terminó.
const LOG_LINES: usize = 500;

/// Cuántos procesos terminados se recuerdan.
///
/// La lista es para mirar qué pasó hace un rato, no un historial: eso ya existe y vive en SQLite.
/// Sin techo, una sesión de un día entero acumularía cientos de fichas que nadie va a leer.
const KEEP_FINISHED: usize = 50;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessKind {
    Maintenance,
    Index,
    Backup,
    Restore,
    Export,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessStatus {
    Running,
    Done,
    Failed,
}

/// Con qué terminó un proceso. Lo que no aplica va vacío: un `VACUUM` no escribió bytes ni filas, y
/// decir «0 filas» sería afirmar algo que nadie midió.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub seconds: f64,
    pub bytes: Option<u64>,
    pub rows: Option<u64>,
    /// Errores que `pg_restore` decidió ignorar en vez de cortar.
    pub ignored_errors: Option<u64>,
    pub path: Option<String>,
    pub database: Option<String>,
}

/// Todo lo que se sabe de un proceso. Es lo que la interfaz dibuja, y lo que le llega tal cual
/// cuando se vuelve a enganchar después de una recarga.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRecord {
    pub task_id: String,
    pub kind: ProcessKind,
    pub profile: ProfileId,
    pub database: String,
    /// Sobre qué corre: `public.pedidos`, el archivo de un backup. Es un rótulo, y lo arma quien
    /// lanza el proceso: el resto de los textos que se muestran también se escriben en la interfaz.
    pub target: String,
    /// El SQL o la línea de comando que se está ejecutando. Se conoce antes de arrancar —es lo
    /// mismo que muestra la vista previa—, así que ya viaja en el récord.
    pub command: String,
    pub log: Vec<String>,
    /// Bytes copiados hasta ahora, en los procesos que mueven un archivo.
    pub progress: Option<u64>,
    pub status: ProcessStatus,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
    pub outcome: Option<Outcome>,
    pub error: Option<ErrorPayload>,
}

/// Lo que viaja por el canal único de procesos.
///
/// El primer mensaje es siempre [`ProcessEvent::Snapshot`]: engancharse y pedir lo que hay son la
/// misma operación, así que no queda una ventana entre las dos por la que se pueda perder un evento.
#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProcessEvent {
    Snapshot {
        records: Vec<ProcessRecord>,
    },
    /// Va en `Box` porque el récord es de lejos lo más grande que viaja: sin él, cada `Log` de una
    /// línea ocuparía en memoria lo mismo que un récord entero.
    Started {
        record: Box<ProcessRecord>,
    },
    Log {
        task_id: String,
        message: String,
    },
    Progress {
        task_id: String,
        bytes: u64,
    },
    Finished {
        task_id: String,
        outcome: Outcome,
    },
    Failed {
        task_id: String,
        error: ErrorPayload,
    },
}

/// Cómo se corta cada clase de proceso.
///
/// Son dos mecanismos distintos y no se pueden confundir: al servidor se le pide que aborte su
/// sentencia —matar la tarea de Rust lo dejaría terminando el `VACUUM` sin nadie escuchando—, y al
/// proceso hijo se le avisa por su canal para que el núcleo lo mate y limpie lo que dejó a medias.
pub enum Cancel {
    Server {
        profile: ProfileId,
        token: CancelToken,
    },
    Child(oneshot::Sender<()>),
}

#[derive(Default)]
struct Inner {
    /// Orden de llegada. El `HashMap` no lo tiene y la lista se muestra por antigüedad.
    order: Vec<String>,
    records: HashMap<String, ProcessRecord>,
    /// Solo de los que siguen corriendo: al terminar, la vía de cancelación deja de existir.
    cancels: HashMap<String, Cancel>,
    /// La ventana enganchada. Es una sola —la aplicación tiene una— y al recargar la reemplaza.
    watcher: Option<Channel<ProcessEvent>>,
}

#[derive(Default)]
pub struct Processes {
    inner: Mutex<Inner>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

impl Inner {
    fn emit(&self, event: ProcessEvent) {
        // Que no haya nadie escuchando es lo normal mientras la ventana recarga: el proceso sigue y
        // el récord queda al día, que es justo de lo que se trata.
        if let Some(watcher) = &self.watcher {
            let _ = watcher.send(event);
        }
    }

    /// Saca los terminados más viejos cuando pasan del techo.
    fn prune(&mut self) {
        let finished: Vec<String> = self
            .order
            .iter()
            .filter(|id| {
                self.records
                    .get(*id)
                    .is_some_and(|record| record.status != ProcessStatus::Running)
            })
            .cloned()
            .collect();

        let extra = finished.len().saturating_sub(KEEP_FINISHED);
        for id in finished.into_iter().take(extra) {
            self.records.remove(&id);
            self.order.retain(|other| other != &id);
        }
    }
}

impl Processes {
    /// Engancha la ventana y le manda de entrada todo lo que hay.
    pub async fn watch(&self, channel: Channel<ProcessEvent>) {
        let mut inner = self.inner.lock().await;
        let records = inner
            .order
            .iter()
            .filter_map(|id| inner.records.get(id).cloned())
            .collect();

        let _ = channel.send(ProcessEvent::Snapshot { records });
        inner.watcher = Some(channel);
    }

    /// Anota un proceso que arranca. Devuelve su identificador, que es con el que se lo cancela.
    pub async fn start(
        &self,
        kind: ProcessKind,
        profile: ProfileId,
        database: String,
        target: String,
        command: String,
        cancel: Cancel,
    ) -> String {
        let task_id = uuid::Uuid::new_v4().to_string();
        let record = ProcessRecord {
            task_id: task_id.clone(),
            kind,
            profile,
            database,
            target,
            command,
            log: Vec::new(),
            progress: None,
            status: ProcessStatus::Running,
            started_ms: now_ms(),
            finished_ms: None,
            outcome: None,
            error: None,
        };

        let mut inner = self.inner.lock().await;
        inner.order.push(task_id.clone());
        inner.records.insert(task_id.clone(), record.clone());
        inner.cancels.insert(task_id.clone(), cancel);
        inner.emit(ProcessEvent::Started {
            record: Box::new(record),
        });

        task_id
    }

    pub async fn log(&self, task_id: &str, message: String) {
        let mut inner = self.inner.lock().await;
        if let Some(record) = inner.records.get_mut(task_id) {
            record.log.push(message.clone());
            let extra = record.log.len().saturating_sub(LOG_LINES);
            if extra > 0 {
                record.log.drain(..extra);
            }
        }
        inner.emit(ProcessEvent::Log {
            task_id: task_id.to_owned(),
            message,
        });
    }

    pub async fn progress(&self, task_id: &str, bytes: u64) {
        let mut inner = self.inner.lock().await;
        if let Some(record) = inner.records.get_mut(task_id) {
            record.progress = Some(bytes);
        }
        inner.emit(ProcessEvent::Progress {
            task_id: task_id.to_owned(),
            bytes,
        });
    }

    pub async fn finish(&self, task_id: &str, outcome: Outcome) {
        let mut inner = self.inner.lock().await;
        inner.cancels.remove(task_id);
        if let Some(record) = inner.records.get_mut(task_id) {
            record.status = ProcessStatus::Done;
            record.finished_ms = Some(now_ms());
            record.outcome = Some(outcome.clone());
        }
        inner.emit(ProcessEvent::Finished {
            task_id: task_id.to_owned(),
            outcome,
        });
        inner.prune();
    }

    pub async fn fail(&self, task_id: &str, error: ErrorPayload) {
        let mut inner = self.inner.lock().await;
        inner.cancels.remove(task_id);
        if let Some(record) = inner.records.get_mut(task_id) {
            record.status = ProcessStatus::Failed;
            record.finished_ms = Some(now_ms());
            record.error = Some(error.clone());
        }
        inner.emit(ProcessEvent::Failed {
            task_id: task_id.to_owned(),
            error,
        });
        inner.prune();
    }

    /// Se lleva la vía de cancelación: cortar dos veces el mismo proceso no tiene sentido, y el
    /// aviso al proceso hijo consume su extremo del canal.
    pub async fn take_cancel(&self, task_id: &str) -> Option<Cancel> {
        self.inner.lock().await.cancels.remove(task_id)
    }

    /// Saca un proceso de la lista. Solo si ya terminó: sacar de la vista uno que sigue corriendo lo
    /// dejaría andando sin nada que lo muestre ni con qué cancelarlo.
    pub async fn remove(&self, task_id: &str) {
        let mut inner = self.inner.lock().await;
        let finished = inner
            .records
            .get(task_id)
            .is_some_and(|record| record.status != ProcessStatus::Running);
        if finished {
            inner.records.remove(task_id);
            inner.order.retain(|other| other != task_id);
        }
    }

    pub async fn clear_finished(&self) {
        let mut inner = self.inner.lock().await;
        inner
            .records
            .retain(|_, record| record.status == ProcessStatus::Running);
        let alive: Vec<String> = inner.records.keys().cloned().collect();
        inner.order.retain(|id| alive.contains(id));
    }
}
