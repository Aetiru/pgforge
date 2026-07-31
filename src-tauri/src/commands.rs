//! Comandos expuestos a la interfaz.
//!
//! Acá no va lógica: cada comando traduce argumentos, delega en `pgforge-core` y devuelve. Lo que
//! haya que pensar vive en el núcleo, donde `pgforge-cli` también lo usa y los tests lo ejercitan
//! sin necesidad de una ventana.

use pgforge_core::caps::MIN_SUPPORTED_VERSION_NUM;
use pgforge_core::conn::store;
use pgforge_core::ddl::{self, Ddl};
use pgforge_core::introspect::{self, TreeNode, TreeOptions};
use pgforge_core::{
    ConnectionProfile, Error, Password, ProfileId, Result, ServerCaps, ServerVersion,
};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

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

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ConnectionProfile>> {
    Ok(state.store.lock().await.profiles().to_vec())
}

/// Guarda el perfil. La contraseña solo llega hasta el almacén de credenciales del sistema, y solo
/// si el usuario pidió recordarla.
#[tauri::command]
pub async fn save_profile(
    state: State<'_, AppState>,
    profile: ConnectionProfile,
    password: Option<String>,
) -> Result<ConnectionProfile> {
    match (&password, profile.save_password) {
        (Some(password), true) => store::store_password(profile.id, &Password::new(password))?,
        (_, false) => store::delete_password(profile.id)?,
        _ => {}
    }

    state.store.lock().await.upsert(profile.clone())?;
    Ok(profile)
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: ProfileId) -> Result<()> {
    state.manager.disconnect(id).await;
    state.store.lock().await.remove(id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connected {
    profile: ConnectionProfile,
    caps: ServerCaps,
}

/// Conecta el perfil. Si no se pasa contraseña se busca la guardada; si tampoco hay, se intenta
/// sin ella (puede haber autenticación por confianza, por certificado o por ident).
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    id: ProfileId,
    password: Option<String>,
) -> Result<Connected> {
    let profile = state
        .store
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| Error::Config("el perfil no existe".to_owned()))?;

    let password = match password {
        Some(password) => Some(Password::new(password)),
        None => store::load_password(id)?,
    };

    let handle = state.manager.connect(profile.clone(), password).await?;
    Ok(Connected {
        profile,
        caps: handle.caps.clone(),
    })
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, id: ProfileId) -> Result<()> {
    state.manager.disconnect(id).await;
    Ok(())
}

#[tauri::command]
pub async fn connected_servers(state: State<'_, AppState>) -> Result<Vec<ProfileId>> {
    Ok(state.manager.connected().await)
}

/// Hijos de un nodo del árbol. Con `parent` en `null` devuelve las bases del servidor.
#[tauri::command]
pub async fn tree_children(
    state: State<'_, AppState>,
    id: ProfileId,
    parent: Option<TreeNode>,
    options: Option<TreeOptions>,
) -> Result<Vec<TreeNode>> {
    let handle = state.manager.require(id).await?;
    introspect::children(&handle, parent.as_ref(), options.unwrap_or_default()).await
}

#[tauri::command]
pub async fn object_ddl(state: State<'_, AppState>, id: ProfileId, node: TreeNode) -> Result<Ddl> {
    let handle = state.manager.require(id).await?;
    ddl::object_ddl(&handle, &node).await
}
