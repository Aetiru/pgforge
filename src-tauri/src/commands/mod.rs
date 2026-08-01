//! Comandos expuestos a la interfaz.
//!
//! Acá no va lógica: cada comando traduce argumentos, delega en `pgforge-core` y devuelve. Lo que
//! haya que pensar vive en el núcleo, donde `pgforge-cli` también lo usa y los tests lo ejercitan
//! sin necesidad de una ventana.

pub mod monitoring;
pub mod schema;
pub mod servers;

use pgforge_core::caps::MIN_SUPPORTED_VERSION_NUM;
use pgforge_core::ServerVersion;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    min_postgres_major: i32,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        min_postgres_major: ServerVersion::from_num(MIN_SUPPORTED_VERSION_NUM).major(),
    }
}
