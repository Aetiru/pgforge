use pgforge_core::{caps::MIN_SUPPORTED_VERSION_NUM, ServerVersion};
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
