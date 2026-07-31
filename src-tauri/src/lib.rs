//! Capa de escritorio: expone el núcleo como comandos de Tauri.

mod commands;
mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            app.manage(state::AppState::new(config_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_profiles,
            commands::save_profile,
            commands::delete_profile,
            commands::connect,
            commands::disconnect,
            commands::connected_servers,
            commands::tree_children,
            commands::object_ddl,
        ])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar la aplicación");
}
