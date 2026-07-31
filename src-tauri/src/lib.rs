//! Capa de escritorio: expone el núcleo como comandos de Tauri.
//!
//! Acá no va lógica de negocio. Todo lo que hace un comando es traducir argumentos, delegar en
//! `pgforge-core` y devolver el resultado; si algo requiere pensar, va en el core, donde
//! `pgforge-cli` también puede usarlo y los tests pueden ejercitarlo sin una ventana.

mod commands;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .run(tauri::generate_context!())
        .expect("no se pudo iniciar la aplicación");
}
