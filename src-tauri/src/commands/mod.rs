//! Comandos expuestos a la interfaz.
//!
//! Acá no va lógica: cada comando traduce argumentos, delega en `pgforge-core` y devuelve. Lo que
//! haya que pensar vive en el núcleo, donde `pgforge-cli` también lo usa y los tests lo ejercitan
//! sin necesidad de una ventana.

pub mod backup;
pub mod compare;
pub mod data;
pub mod ddl;
pub mod monitoring;
pub mod query;
pub mod schema;
pub mod servers;
pub mod settings;
pub mod tasks;
pub mod update;

use pgforge_core::caps::MIN_SUPPORTED_VERSION_NUM;
use pgforge_core::ddl::table::Statement;
use pgforge_core::sql::history::{NewEntry, Source};
use pgforge_core::{ProfileId, Result, ServerVersion};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    min_postgres_major: i32,
    /// Dónde quedan los archivos de registro. Se lo dice a la interfaz para que el usuario pueda
    /// encontrarlos sin tener que saber dónde los pone cada sistema operativo.
    log_dir: Option<String>,
}

/// Anota en el historial lo que un diálogo acaba de aplicar, y devuelve lo que devolvió la operación.
///
/// El historial dejó de ser «lo que escribí en el editor»: un `ALTER TABLE` salido de un diálogo no
/// quedaba en ningún lado que se pudiera consultar después, y esa es la pregunta del día siguiente
/// —qué cambió, cuándo y contra qué servidor—. Se anota **el SQL que se ejecutó**, que es el mismo
/// que muestra la vista previa, no una descripción del cambio.
///
/// Lo que falle al anotar no toca el resultado: la operación ya corrió contra el servidor y
/// perderla por un problema del archivo local sería mucho peor que perder la anotación.
pub async fn record_applied<T>(
    state: &AppState,
    id: ProfileId,
    database: &str,
    sql: String,
    operation: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default();
    let started = std::time::Instant::now();

    let outcome = operation.await;

    let entry = NewEntry {
        source: Source::Dialog,
        profile_id: id.to_string(),
        database: database.to_owned(),
        sql,
        started_at,
        seconds: started.elapsed().as_secs_f64(),
        row_count: None,
        error: outcome.as_ref().err().map(ToString::to_string),
    };
    // Se descarta a propósito: la operación ya corrió contra el servidor, y devolver el error del
    // archivo local en su lugar diría que falló algo que sí funcionó.
    let _ = state.history.lock().await.record(&entry);

    outcome
}

/// Lo que tienen en común las sentencias que arman los distintos módulos del núcleo.
///
/// Cada dominio tiene su propio `Statement` —el de datos lleva además los parámetros del `UPDATE`—,
/// y lo único que necesita el historial es el texto. Un rasgo local evita elegir entre unificarlos
/// en el núcleo o repetir el mismo `map` en cada comando.
pub trait HasSql {
    fn sql(&self) -> &str;
}

impl HasSql for Statement {
    fn sql(&self) -> &str {
        &self.sql
    }
}

impl HasSql for pgforge_core::data::Statement {
    fn sql(&self) -> &str {
        &self.sql
    }
}

/// El texto de un lote de sentencias, tal como se ejecuta y como lo muestra la vista previa.
pub fn sql_of(statements: &[impl HasSql]) -> String {
    statements
        .iter()
        .map(HasSql::sql)
        .collect::<Vec<_>>()
        .join(";\n")
}

#[tauri::command]
pub fn app_info(app: tauri::AppHandle) -> AppInfo {
    use tauri::Manager;

    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        min_postgres_major: ServerVersion::from_num(MIN_SUPPORTED_VERSION_NUM).major(),
        log_dir: app
            .path()
            .app_log_dir()
            .ok()
            .map(|path| path.display().to_string()),
    }
}
