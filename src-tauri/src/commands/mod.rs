//! Comandos expuestos a la interfaz.
//!
//! Acá no va lógica: cada comando traduce argumentos, delega en `pgforge-core` y devuelve. Lo que
//! haya que pensar vive en el núcleo, donde `pgforge-cli` también lo usa y los tests lo ejercitan
//! sin necesidad de una ventana.

pub mod backup;
pub mod data;
pub mod ddl;
pub mod monitoring;
pub mod query;
pub mod schema;
pub mod servers;
pub mod settings;

use pgforge_core::caps::MIN_SUPPORTED_VERSION_NUM;
use pgforge_core::ServerVersion;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    min_postgres_major: i32,
    /// Dónde quedan los archivos de registro. Se lo dice a la interfaz para que el usuario pueda
    /// encontrarlos sin tener que saber dónde los pone cada sistema operativo.
    log_dir: Option<String>,
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
