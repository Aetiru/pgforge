//! Archivos de la aplicación que solo debería leer su dueño.
//!
//! `connections.json` trae hosts, usuarios y rutas de claves SSH, y el historial guarda SQL: en un
//! equipo compartido, el permiso por omisión (0644) deja leerlos a cualquier otro usuario. En
//! Windows el directorio de configuración ya hereda permisos del perfil del usuario, así que allá
//! no hay nada que ajustar.

use std::path::Path;

/// Escribe `contents` en `path` con permisos 0600 (en Unix) desde que el archivo existe.
///
/// El modo se fija al crearlo y no con un `chmod` posterior: de la otra forma hay un instante en
/// que el contenido ya está en disco y es legible por otros. Si el archivo ya existía con otro
/// modo, `set_permissions` lo corrige igual.
pub(crate) fn write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(contents)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)
    }
}

/// Deja `path` creado y con permisos 0600 (en Unix) antes de que SQLite lo abra.
///
/// SQLite crea el archivo con el modo por omisión, y los archivos `-wal`/`-shm` heredan el del
/// principal, así que alcanza con fijarlo acá. `:memory:` no es un archivo y se deja pasar.
pub(crate) fn restrict(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        if path.as_os_str() == ":memory:" || path.as_os_str().is_empty() {
            return Ok(());
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn modo(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("pgforge-private-{}-{name}", std::process::id()))
    }

    #[test]
    fn escribe_con_permisos_solo_del_dueno() {
        let path = temp("write");
        let _ = std::fs::remove_file(&path);
        write(&path, b"{}").unwrap();
        assert_eq!(modo(&path), 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"{}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn un_archivo_existente_con_permisos_amplios_se_cierra() {
        let path = temp("amplio");
        std::fs::write(&path, b"x").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write(&path, b"y").unwrap();
        assert_eq!(modo(&path), 0o600);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        restrict(&path).unwrap();
        assert_eq!(modo(&path), 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"y");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn las_bases_sqlite_se_crean_con_permisos_solo_del_dueno() {
        let path = temp("history.db");
        let _ = std::fs::remove_file(&path);
        let _store = crate::sql::history::HistoryStore::open(&path).unwrap();
        assert_eq!(modo(&path), 0o600);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
