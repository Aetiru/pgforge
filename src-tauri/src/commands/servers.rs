//! Perfiles de conexión y estado de los servidores.

use pgforge_core::conn::import::{self, Candidate};
use pgforge_core::conn::{store, tunnel, HostKeyPolicy};
use pgforge_core::{ConnectionProfile, Error, Password, ProfileId, Result, ServerCaps};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

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

/// Busca servidores ya configurados en otras herramientas: los archivos de `libpq` y DBeaver.
///
/// Solo lee y devuelve candidatos; guardar cuáles es una decisión del usuario, que los ve antes. Sin
/// contraseñas: `.pgpass` las tiene en texto plano y traerlas sería hacer una copia que nadie pidió
/// (ver `conn::import`).
#[tauri::command]
pub async fn import_scan(app: AppHandle) -> Result<Vec<Candidate>> {
    let home = app
        .path()
        .home_dir()
        .map_err(|e| Error::Config(format!("no se pudo ubicar la carpeta del usuario: {e}")))?;
    let data = app.path().data_dir().ok();

    let paths = import::sources(&home, data.as_deref());
    import::scan(&paths)
}

/// Busca servidores en TODOS los proyectos de un espacio de trabajo de DBeaver (`root`), no solo el
/// que `import_scan` conoce de memoria. Cada proyecto es una carpeta de primer nivel bajo `root`; la
/// carpeta resultante en pgforge lleva el nombre del proyecto adelante, para que dos proyectos con
/// una subcarpeta igual no se pisen al quedar como carpetas raíz hermanas.
#[tauri::command]
pub async fn import_scan_workspace(root: String) -> Result<Vec<Candidate>> {
    let root = std::path::PathBuf::from(root);
    let mut out: Vec<Candidate> = Vec::new();

    for path in import::dbeaver_workspace_sources(&root) {
        // Un archivo ilegible (permisos, corrupto) no frena el resto del escaneo: se saltea y se
        // sigue con los demás proyectos, igual que hace `scan()`.
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // El nombre del proyecto es la carpeta padre de `.dbeaver` en la ruta del archivo.
        let project = path
            .parent()
            .and_then(|dbeaver_dir| dbeaver_dir.parent())
            .and_then(|project_dir| project_dir.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let source = path.display().to_string();

        for candidate in import::dbeaver_scoped(&text, &source, &project) {
            // Mismo criterio de "repetido" que usa `scan()` (host, puerto, usuario y base), más la
            // carpeta: `dbeaver_scoped` la prefija con el nombre del proyecto, así que dos servidores
            // iguales que viven en proyectos DBeaver distintos no son el mismo servidor y no hay que
            // fundirlos en uno — algo común, porque DBeaver deja `user` vacío muy seguido y
            // "localhost:5432/postgres" se repite en cualquier espacio de trabajo.
            let repetido = out.iter().any(|item| {
                item.host == candidate.host
                    && item.port == candidate.port
                    && item.user == candidate.user
                    && item.database == candidate.database
                    && item.group == candidate.group
            });
            if !repetido {
                out.push(candidate);
            }
        }
    }

    Ok(out)
}

/// Guarda los candidatos elegidos como servidores nuevos y devuelve los perfiles creados.
#[tauri::command]
pub async fn import_apply(
    state: State<'_, AppState>,
    candidates: Vec<Candidate>,
    group: Option<String>,
) -> Result<Vec<ConnectionProfile>> {
    let mut out = Vec::new();
    let mut store = state.store.lock().await;

    for candidate in &candidates {
        let mut profile = candidate.profile();
        // La carpeta elegida manda; si no se eligió ninguna, se respeta la que el servidor tenía en
        // la otra herramienta. Veinte servidores sueltos en la raíz del árbol es peor que no
        // haberlos importado.
        if let Some(group) = group.as_ref().filter(|name| !name.trim().is_empty()) {
            profile.group = Some(group.clone());
        }
        store.upsert(profile.clone())?;
        out.push(profile);
    }

    Ok(out)
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: ProfileId) -> Result<()> {
    // Borrar el servidor apaga su monitoreo en cualquier ventana que lo tuviera abierto, no solo en
    // la que pidió el borrado.
    state.monitors.lock().await.retain(|(pid, _), _| *pid != id);
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
    // servidor que la interfaz ya dio por desconectado. Se sacan todas las ventanas que lo tenían
    // abierto, no solo la que pidió desconectar.
    state.monitors.lock().await.retain(|(pid, _), _| *pid != id);
    state.manager.disconnect(id).await;
    Ok(())
}

#[tauri::command]
pub async fn connected_servers(state: State<'_, AppState>) -> Result<Vec<ProfileId>> {
    Ok(state.manager.connected().await)
}
