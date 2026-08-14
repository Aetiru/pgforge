//! Aviso de versión nueva.
//!
//! La comparación vive en `pgforge_core::update`; acá solo se le pasa la versión de la aplicación
//! —la del binario de escritorio, que es la que ve el usuario— y se abre la página de la release en
//! el navegador del sistema.

use pgforge_core::update::{self, UpdateCheck};
use pgforge_core::{Error, Result};

#[tauri::command]
pub async fn update_check() -> Result<UpdateCheck> {
    update::check(env!("CARGO_PKG_VERSION")).await
}

/// Abre la página de una release en el navegador del sistema.
///
/// Se acepta cualquier dirección pero se exige que sea del repositorio: el único que llama es el
/// cartel de versión nueva, y así una respuesta rara de la API —o de algo que se haga pasar por
/// ella— no puede terminar abriendo lo que quiera en la máquina del usuario.
#[tauri::command]
pub async fn update_open(app: tauri::AppHandle, url: String) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;

    let prefix = format!("{}/releases", env!("CARGO_PKG_REPOSITORY"));
    if !url.starts_with(&prefix) {
        return Err(Error::UpdateCheck(format!(
            "la dirección {url} no es la de una release de pgforge"
        )));
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| Error::UpdateCheck(format!("no se pudo abrir el navegador: {e}")))
}
