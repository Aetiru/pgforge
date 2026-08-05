//! Perfiles de conexión y estado de los servidores.

use pgforge_core::conn::{store, tunnel, HostKeyPolicy};
use pgforge_core::{ConnectionProfile, Error, Password, ProfileId, Result, ServerCaps};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// Traduce la confirmación de la interfaz en una política de verificación de host key. `Some(true)`
/// llega solo después de que el usuario aceptó la huella; en cualquier otro caso se es estricto.
fn host_key_policy(trust_host_key: Option<bool>) -> HostKeyPolicy {
    if trust_host_key == Some(true) {
        HostKeyPolicy::TrustOnFirstUse
    } else {
        HostKeyPolicy::Strict
    }
}

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ConnectionProfile>> {
    Ok(state.store.lock().await.profiles().to_vec())
}

/// Las carpetas en las que están repartidos los servidores guardados.
#[tauri::command]
pub async fn list_groups(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok(state.store.lock().await.groups())
}

/// Renombra una carpeta de conexiones, o la deshace si `to` es `None`.
#[tauri::command]
pub async fn rename_group(
    state: State<'_, AppState>,
    from: String,
    to: Option<String>,
) -> Result<usize> {
    state.store.lock().await.rename_group(&from, to.as_deref())
}

/// Guarda el perfil. La contraseña solo llega hasta el almacén de credenciales del sistema, y solo
/// si el usuario pidió recordarla.
#[tauri::command]
pub async fn save_profile(
    state: State<'_, AppState>,
    profile: ConnectionProfile,
    password: Option<String>,
    ssh_password: Option<String>,
) -> Result<ConnectionProfile> {
    match (&password, profile.save_password) {
        (Some(password), true) => store::store_password(profile.id, &Password::new(password))?,
        (_, false) => store::delete_password(profile.id)?,
        _ => {}
    }

    // El secreto SSH (contraseña del bastión o frase de la clave) se guarda si el usuario lo escribió;
    // si el perfil dejó de tener túnel, se limpia el que hubiera quedado para no dejar basura invisible.
    match (&ssh_password, profile.tunnel.is_some()) {
        (Some(secret), true) => store::store_ssh_secret(profile.id, &Password::new(secret))?,
        (_, false) => store::delete_ssh_secret(profile.id)?,
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
///
/// Con un túnel SSH, `trustHostKey` en `true` llega solo tras confirmar la huella del bastión: la
/// primera conexión a un host desconocido devuelve un error `sshHostKey` con la huella, y la interfaz
/// vuelve a llamar con esta bandera para aceptarla y recordarla en `known_hosts`.
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    id: ProfileId,
    password: Option<String>,
    ssh_password: Option<String>,
    trust_host_key: Option<bool>,
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

    let ssh_secret = match ssh_password {
        Some(secret) => Some(Password::new(secret)),
        None => store::load_ssh_secret(id)?,
    };

    let handle = state
        .manager
        .connect_with_ssh(
            profile.clone(),
            password,
            ssh_secret,
            host_key_policy(trust_host_key),
        )
        .await?;
    Ok(Connected {
        profile,
        caps: handle.caps.clone(),
    })
}

/// Prueba el túnel SSH del perfil sin conectar a la base ni guardar nada: útil en el diálogo de
/// conexión antes de aceptar. Devuelve el mismo error `sshHostKey` que `connect` si la clave del
/// bastión no está verificada, para reusar el mismo flujo de confirmación.
#[tauri::command]
pub async fn ssh_test(
    profile: ConnectionProfile,
    ssh_password: Option<String>,
    trust_host_key: Option<bool>,
) -> Result<()> {
    let spec = profile
        .tunnel
        .as_ref()
        .ok_or_else(|| Error::Config("el perfil no tiene un túnel SSH configurado".to_owned()))?;

    let ssh_secret = match ssh_password {
        Some(secret) => Some(Password::new(secret)),
        None => store::load_ssh_secret(profile.id)?,
    };

    tunnel::test_connection(
        spec,
        ssh_secret.as_ref(),
        &profile.host,
        profile.port,
        host_key_policy(trust_host_key),
    )
    .await
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
