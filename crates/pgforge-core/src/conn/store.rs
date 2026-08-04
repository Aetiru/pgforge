//! Almacén de perfiles.
//!
//! Los perfiles van a un archivo JSON legible; las contraseñas van al almacén de credenciales del
//! sistema (Credential Manager, Keychain, Secret Service) y nunca tocan el disco de la aplicación.

use std::path::{Path, PathBuf};

use super::profile::{normalize_group, ConnectionProfile, ProfileId};
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

    /// Las carpetas existentes, ordenadas como se muestran. Salen de los perfiles: no hay una lista
    /// de carpetas aparte que pueda quedar desincronizada con ellos.
    pub fn groups(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .profiles
            .iter()
            .filter_map(|profile| profile.group.clone())
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        names
    }

    /// Agrega el perfil, o reemplaza el existente con el mismo identificador.
    pub fn upsert(&mut self, mut profile: ConnectionProfile) -> Result<()> {
        profile.group = normalize_group(profile.group.as_deref());

        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile,
            None => self.profiles.push(profile),
        }
        self.persist()
    }

    /// Cambia de nombre una carpeta, o la deshace si `to` es `None`, dejando sus servidores sueltos.
    /// Devuelve cuántos perfiles se movieron.
    ///
    /// Va acá y no en quien la muestra porque toca todos los perfiles de la carpeta y tiene que
    /// escribirlos de una sola vez: renombrar de a uno dejaría la lista partida en dos carpetas si
    /// algo falla a mitad de camino.
    pub fn rename_group(&mut self, from: &str, to: Option<&str>) -> Result<usize> {
        let to = normalize_group(to);
        let mut moved = 0;

        for profile in &mut self.profiles {
            if profile.group.as_deref() == Some(from) {
                profile.group = to.clone();
                moved += 1;
            }
        }

        if moved > 0 {
            self.persist()?;
        }
        Ok(moved)
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

    /// Guarda los perfiles indicados, cada uno en la carpeta que se le pasa.
    fn store_with(name: &str, groups: &[(&str, Option<&str>)]) -> ProfileStore {
        let path = temp_path(name);
        let _ = std::fs::remove_file(&path);

        let mut store = ProfileStore::load(&path).unwrap();
        for (server, group) in groups {
            let mut profile = ConnectionProfile::new(*server, "localhost", "postgres");
            profile.group = group.map(str::to_owned);
            store.upsert(profile).unwrap();
        }
        store
    }

    #[test]
    fn las_carpetas_salen_de_los_perfiles_y_no_se_repiten() {
        let store = store_with(
            "carpetas",
            &[
                ("uno", Some("Producción")),
                ("dos", Some("desarrollo")),
                ("tres", Some("Producción")),
                ("cuatro", None),
            ],
        );

        // Ordenadas sin distinguir mayúsculas: si no, «Producción» iría antes que «desarrollo».
        assert_eq!(store.groups(), vec!["desarrollo", "Producción"]);
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn el_nombre_de_la_carpeta_se_normaliza_al_guardar() {
        let store = store_with(
            "normalizar",
            &[("uno", Some("  Producción ")), ("dos", Some("   "))],
        );

        assert_eq!(store.groups(), vec!["Producción"]);
        // Una carpeta en blanco no es una carpeta: el servidor queda suelto.
        assert!(store.profiles()[1].group.is_none());
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn renombrar_una_carpeta_mueve_todos_sus_servidores() {
        let mut store = store_with(
            "renombrar-carpeta",
            &[
                ("uno", Some("vieja")),
                ("dos", Some("vieja")),
                ("tres", Some("otra")),
            ],
        );

        assert_eq!(store.rename_group("vieja", Some("nueva")).unwrap(), 2);
        assert_eq!(store.groups(), vec!["nueva", "otra"]);

        // Y el cambio quedó en disco, no solo en memoria.
        let releido = ProfileStore::load(store.path()).unwrap();
        assert_eq!(releido.groups(), vec!["nueva", "otra"]);
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn deshacer_una_carpeta_deja_sus_servidores_sueltos() {
        let mut store = store_with(
            "deshacer-carpeta",
            &[("uno", Some("vieja")), ("dos", Some("otra"))],
        );

        assert_eq!(store.rename_group("vieja", None).unwrap(), 1);
        assert_eq!(store.groups(), vec!["otra"]);
        assert!(store.profiles()[0].group.is_none());
        let _ = std::fs::remove_file(store.path());
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
