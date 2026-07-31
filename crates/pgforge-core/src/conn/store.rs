//! Almacén de perfiles.
//!
//! Los perfiles van a un archivo JSON legible; las contraseñas van al almacén de credenciales del
//! sistema (Credential Manager, Keychain, Secret Service) y nunca tocan el disco de la aplicación.

use std::path::{Path, PathBuf};

use super::profile::{ConnectionProfile, ProfileId};
use super::secret::Password;
use crate::error::{Error, Result};

/// Nombre del servicio bajo el que se registran las credenciales en el almacén del sistema.
const KEYRING_SERVICE: &str = "pgforge";

#[derive(Debug)]
pub struct ProfileStore {
    path: PathBuf,
    profiles: Vec<ConnectionProfile>,
}

impl ProfileStore {
    /// Lee los perfiles del archivo indicado. Un archivo inexistente es un almacén vacío, no un
    /// error: es el estado normal en el primer arranque.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let profiles = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                Error::Config(format!(
                    "el archivo de conexiones {} está corrupto: {e}",
                    path.display()
                ))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(Error::Io(e)),
        };
        Ok(Self { path, profiles })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn profiles(&self) -> &[ConnectionProfile] {
        &self.profiles
    }

    pub fn get(&self, id: ProfileId) -> Option<&ConnectionProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Agrega el perfil, o reemplaza el existente con el mismo identificador.
    pub fn upsert(&mut self, profile: ConnectionProfile) -> Result<()> {
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile,
            None => self.profiles.push(profile),
        }
        self.persist()
    }

    /// Elimina el perfil y su contraseña guardada. Dejar la credencial huérfana en el almacén del
    /// sistema sería basura invisible que el usuario no puede limpiar desde la aplicación.
    pub fn remove(&mut self, id: ProfileId) -> Result<()> {
        self.profiles.retain(|p| p.id != id);
        delete_password(id)?;
        self.persist()
    }

    /// Escritura atómica: se escribe un archivo temporal y recién ahí se reemplaza el definitivo,
    /// para que un corte a mitad de camino no deje la lista de conexiones truncada.
    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&self.profiles)
            .map_err(|e| Error::Config(format!("no se pudo serializar la lista: {e}")))?;

        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn entry(id: ProfileId) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, &id.to_string())?)
}

pub fn store_password(id: ProfileId, password: &Password) -> Result<()> {
    entry(id)?.set_password(password.expose())?;
    Ok(())
}

/// Devuelve `None` si el perfil no tiene contraseña guardada.
pub fn load_password(id: ProfileId) -> Result<Option<Password>> {
    match entry(id)?.get_password() {
        Ok(value) => Ok(Some(Password::new(value))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Credentials(e.to_string())),
    }
}

pub fn delete_password(id: ProfileId) -> Result<()> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Credentials(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("pgforge-test-{}-{name}.json", std::process::id()));
        path
    }

    #[test]
    fn un_archivo_inexistente_es_un_almacen_vacio() {
        let store = ProfileStore::load(temp_path("inexistente")).unwrap();
        assert!(store.profiles().is_empty());
    }

    #[test]
    fn guarda_y_relee_los_perfiles() {
        let path = temp_path("ida-y-vuelta");
        let _ = std::fs::remove_file(&path);

        let mut store = ProfileStore::load(&path).unwrap();
        let mut profile = ConnectionProfile::new("local 16", "localhost", "postgres");
        profile.port = 5432;
        let id = profile.id;
        store.upsert(profile).unwrap();

        let releido = ProfileStore::load(&path).unwrap();
        assert_eq!(releido.profiles().len(), 1);
        assert_eq!(releido.get(id).unwrap().name, "local 16");

        // El archivo en disco no debe tener un campo de contraseña. `savePassword` sí está: es una
        // preferencia, no una credencial.
        let contenido = std::fs::read_to_string(&path).unwrap();
        assert!(
            !contenido.contains("\"password\""),
            "apareció un campo de contraseña en el archivo: {contenido}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn actualizar_no_duplica() {
        let path = temp_path("upsert");
        let _ = std::fs::remove_file(&path);

        let mut store = ProfileStore::load(&path).unwrap();
        let mut profile = ConnectionProfile::new("antes", "localhost", "postgres");
        store.upsert(profile.clone()).unwrap();

        profile.name = "después".to_owned();
        store.upsert(profile.clone()).unwrap();

        assert_eq!(store.profiles().len(), 1);
        assert_eq!(store.get(profile.id).unwrap().name, "después");

        let _ = std::fs::remove_file(&path);
    }
}
