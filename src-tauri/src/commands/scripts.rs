//! Scripts: los `.sql` de cada conexión, guardados solos en `scripts/`, una carpeta por conexión.
//!
//! Toda ruta que llega de la interfaz pasa por [`pgforge_core::scripts::within`] antes de tocar
//! disco: sin esa guarda, una ruta armada a mano podría escribir o borrar cualquier archivo fuera
//! de la carpeta de scripts.

use std::path::PathBuf;

use pgforge_core::{Error, ImportReport, ProfileId, Result, ScriptFolder};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn scripts_tree(state: State<'_, AppState>) -> Result<Vec<ScriptFolder>> {
    pgforge_core::scripts::tree(&state.scripts_root)
}

/// Para el botón «Abrir la carpeta» de la interfaz: no toca disco, solo informa dónde está.
///
/// Devuelve `Result` aunque no pueda fallar: un comando `async` con `State` como argumento lo exige
/// (referencias que no viven `'static`), mismo motivo que el resto de los comandos de este archivo.
#[tauri::command]
pub async fn scripts_root_path(state: State<'_, AppState>) -> Result<String> {
    Ok(state.scripts_root.display().to_string())
}

/// Devuelve la ruta completa (no solo el nombre) del próximo `scriptN.sql` libre para esa conexión.
///
/// Arma la ruta acá, con `folder_name` ya aplicado, para que la interfaz no tenga que repetir ese
/// saneado en TypeScript: la ruta que se guarda en `QueryTab.scriptPath` viaja directo a
/// `script_write`/`script_rename` sin que la ventana reconstruya el nombre de la carpeta.
#[tauri::command]
pub async fn script_new_name(state: State<'_, AppState>, profile_id: ProfileId) -> Result<String> {
    let folder = {
        let store = state.store.lock().await;
        let profile = store
            .get(profile_id)
            .ok_or_else(|| Error::Config("perfil desconocido".to_owned()))?;
        pgforge_core::scripts::folder_path(&state.scripts_root, &profile.name)
    };

    let existing = match std::fs::read_dir(&folder) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.ends_with(".sql").then_some(name)
            })
            .collect::<Vec<_>>(),
        // La carpeta de la conexión todavía no existe: es el caso normal antes del primer script.
        Err(_) => Vec::new(),
    };

    let name = pgforge_core::scripts::next_name(&existing);
    Ok(folder.join(name).display().to_string())
}

#[tauri::command]
pub async fn script_read(state: State<'_, AppState>, path: String) -> Result<String> {
    let path = pgforge_core::scripts::within(&state.scripts_root, &PathBuf::from(path))?;
    pgforge_core::scripts::read(&path)
}

#[tauri::command]
pub async fn script_write(state: State<'_, AppState>, path: String, content: String) -> Result<()> {
    let path = pgforge_core::scripts::within(&state.scripts_root, &PathBuf::from(path))?;
    pgforge_core::scripts::write(&path, &content)
}

#[tauri::command]
pub async fn script_rename(state: State<'_, AppState>, old: String, new: String) -> Result<()> {
    let old = pgforge_core::scripts::within(&state.scripts_root, &PathBuf::from(old))?;
    let new = pgforge_core::scripts::within(&state.scripts_root, &PathBuf::from(new))?;
    pgforge_core::scripts::rename(&old, &new)
}

#[tauri::command]
pub async fn script_delete(state: State<'_, AppState>, path: String) -> Result<()> {
    let path = pgforge_core::scripts::within(&state.scripts_root, &PathBuf::from(path))?;
    pgforge_core::scripts::delete(&path)
}

#[tauri::command]
pub async fn script_create_folder(state: State<'_, AppState>, path: String) -> Result<()> {
    let path = pgforge_core::scripts::within(&state.scripts_root, &PathBuf::from(path))?;
    pgforge_core::scripts::create_folder(&path)
}

/// Importa los `.sql` de una carpeta elegida por el usuario hacia la carpeta de una conexión.
///
/// `src` no pasa por `within`: está fuera de la raíz de scripts a propósito, es de ahí de donde se
/// importa.
#[tauri::command]
pub async fn scripts_import_folder(
    state: State<'_, AppState>,
    profile_id: ProfileId,
    src: String,
) -> Result<ImportReport> {
    let dest = {
        let store = state.store.lock().await;
        let profile = store
            .get(profile_id)
            .ok_or_else(|| Error::Config("perfil desconocido".to_owned()))?;
        pgforge_core::scripts::folder_path(&state.scripts_root, &profile.name)
    };
    std::fs::create_dir_all(&dest)?;

    pgforge_core::scripts::import_folder(&PathBuf::from(src), &dest)
}
