//! Perfiles de conexión y estado de los servidores.

use pgforge_core::conn::store;
use pgforge_core::{ConnectionProfile, Error, Password, ProfileId, Result, ServerCaps};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

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
    state.monitors.lock().await.remove(&id);
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
    // El monitoreo tiene su propia conexión: hay que cerrarla también o queda consultando un
    // servidor que la interfaz ya dio por desconectado.
    state.monitors.lock().await.remove(&id);
    state.manager.disconnect(id).await;
    Ok(())
}

#[tauri::command]
pub async fn connected_servers(state: State<'_, AppState>) -> Result<Vec<ProfileId>> {
    Ok(state.manager.connected().await)
}
