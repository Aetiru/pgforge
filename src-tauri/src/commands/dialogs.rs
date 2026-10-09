//! Diálogos nativos de archivos, abiertos desde Rust.
//!
//! Son la **única** fuente del registro de rutas autorizadas (ver [`crate::paths`]): lo que el
//! usuario elige acá es lo que después pueden leer o escribir `sql_read_file`, `data_export_run` y
//! compañía. Por eso el diálogo se abre del lado de Rust y no con el complemento de JavaScript:
//! desde el webview no hay forma de fabricar una ruta «elegida».

use std::path::{Path, PathBuf};

use pgforge_core::{Error, Result};
use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DialogFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogOptions {
    pub title: Option<String>,
    /// Solo orienta dónde abre el diálogo y qué nombre propone; no autoriza nada por sí mismo.
    pub default_path: Option<String>,
    #[serde(default)]
    pub directory: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub filters: Vec<DialogFilter>,
}

fn build<R: tauri::Runtime>(
    mut dialog: FileDialogBuilder<R>,
    options: &DialogOptions,
) -> FileDialogBuilder<R> {
    if let Some(title) = &options.title {
        dialog = dialog.set_title(title);
    }
    for filter in &options.filters {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(&filter.name, &extensions);
    }
    if let Some(default) = &options.default_path {
        let default = Path::new(default);
        if let Some(parent) = default.parent().filter(|parent| parent.is_dir()) {
            dialog = dialog.set_directory(parent);
        }
        if let Some(name) = default.file_name().and_then(|name| name.to_str()) {
            dialog = dialog.set_file_name(name);
        }
    }
    dialog
}

fn to_path(path: tauri_plugin_dialog::FilePath) -> Result<PathBuf> {
    path.into_path()
        .map_err(|err| Error::Config(format!("el diálogo devolvió una ruta inválida: {err}")))
}

/// Abre el diálogo de «elegir» (archivo, varios archivos o carpeta) y autoriza lo elegido.
///
/// Devuelve la lista vacía si el usuario cancela. Corre en un hilo aparte: el diálogo bloquea hasta
/// que se cierra, y bloquear el hilo del runtime congelaría el resto de los comandos.
#[tauri::command]
pub async fn dialog_open(
    app: AppHandle,
    state: State<'_, AppState>,
    options: DialogOptions,
) -> Result<Vec<String>> {
    let directory = options.directory;
    let multiple = options.multiple;
    let chosen = tokio::task::spawn_blocking(move || {
        let dialog = build(app.dialog().file(), &options);
        if directory {
            dialog.blocking_pick_folder().map(|path| vec![path])
        } else if multiple {
            dialog.blocking_pick_files()
        } else {
            dialog.blocking_pick_file().map(|path| vec![path])
        }
    })
    .await
    .map_err(|err| Error::Config(format!("el diálogo falló: {err}")))?;

    let mut out = Vec::new();
    for path in chosen.unwrap_or_default() {
        let path = to_path(path)?;
        state.paths.authorize(&path)?;
        out.push(path.display().to_string());
    }
    Ok(out)
}

/// Abre el diálogo de «guardar como» y autoriza el destino. `None` si el usuario cancela.
#[tauri::command]
pub async fn dialog_save(
    app: AppHandle,
    state: State<'_, AppState>,
    options: DialogOptions,
) -> Result<Option<String>> {
    let chosen = tokio::task::spawn_blocking(move || {
        build(app.dialog().file(), &options).blocking_save_file()
    })
    .await
    .map_err(|err| Error::Config(format!("el diálogo falló: {err}")))?;

    match chosen {
        Some(path) => {
            let path = to_path(path)?;
            state.paths.authorize(&path)?;
            Ok(Some(path.display().to_string()))
        }
        None => Ok(None),
    }
}
