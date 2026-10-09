//! Rutas del disco que el usuario eligió en un diálogo nativo.
//!
//! La interfaz corre en un webview: si algo ajeno se ejecutara ahí, podría pedirle a Rust que lea o
//! escriba cualquier archivo con los permisos del usuario. Por eso los comandos que reciben una
//! ruta (`sql_read_file`, `data_export_run`, …) no confían en la cadena que llega: la comparan con
//! este registro, que **solo** alimentan los comandos de diálogo (`dialog_open`/`dialog_save`), que
//! abre Rust y no la interfaz. No existe —y no debe existir— un comando que registre una ruta que
//! venga del webview: sería la misma puerta abierta con un paso más.

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use pgforge_core::scripts::resolve;
use pgforge_core::{Error, Result};

#[derive(Default)]
pub struct AllowedPaths {
    /// Canonicalizadas al registrarlas. Una carpeta autoriza a todo lo que cuelga de ella.
    allowed: Mutex<Vec<PathBuf>>,
}

impl AllowedPaths {
    /// Registra lo que eligió el usuario. Solo lo llaman los comandos de diálogo.
    pub fn authorize(&self, path: &Path) -> Result<()> {
        let path = canonical(path)?;
        let mut allowed = self.allowed.lock().expect("registro de rutas envenenado");
        if !allowed.contains(&path) {
            allowed.push(path);
        }
        Ok(())
    }

    /// Devuelve la ruta canónica si `path` es una elegida o cuelga de una carpeta elegida.
    ///
    /// Se compara canonicalizada y por componentes, no por texto: `/elegida/../etc`, una ruta con
    /// `..` escondido en un tramo que todavía no existe, o un enlace simbólico dentro de la carpeta
    /// que apunta afuera, no pueden pasar por hijos de ella.
    pub fn check(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        let denied = || {
            Error::Permission(format!(
                "la ruta {} no fue elegida en un diálogo de archivos: elegila de nuevo desde la \
                 aplicación",
                path.display()
            ))
        };
        let resolved = canonical(path).map_err(|_| denied())?;
        let allowed = self.allowed.lock().expect("registro de rutas envenenado");
        if allowed.iter().any(|root| resolved.starts_with(root)) {
            Ok(resolved)
        } else {
            Err(denied())
        }
    }
}

fn canonical(path: &Path) -> Result<PathBuf> {
    // `resolve` conserva un `..` que cae en un tramo inexistente, y ahí `starts_with` se dejaría
    // engañar. Un diálogo nunca devuelve uno, así que se rechaza en vez de normalizarlo.
    if path.components().any(|part| part == Component::ParentDir) {
        return Err(Error::Config(format!(
            "la ruta {} contiene «..»",
            path.display()
        )));
    }
    resolve(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pgforge-paths-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn text(path: &Path) -> &str {
        path.to_str().unwrap()
    }

    #[test]
    fn una_ruta_no_autorizada_se_rechaza() {
        let base = dir("ninguna");
        let paths = AllowedPaths::default();
        let error = paths.check(text(&base.join("a.sql"))).unwrap_err();
        assert!(matches!(error, Error::Permission(_)));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn un_archivo_elegido_aunque_todavia_no_exista_se_acepta() {
        let base = dir("archivo");
        let paths = AllowedPaths::default();
        let destino = base.join("salida.sql");
        paths.authorize(&destino).unwrap();
        assert!(paths.check(text(&destino)).is_ok());
        assert!(paths.check(text(&base.join("otro.sql"))).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn una_carpeta_elegida_autoriza_a_sus_descendientes() {
        let base = dir("carpeta");
        std::fs::create_dir_all(base.join("sub")).unwrap();
        let paths = AllowedPaths::default();
        paths.authorize(&base).unwrap();
        assert!(paths.check(text(&base.join("sub").join("a.sql"))).is_ok());
        assert!(paths.check(text(&base.join("nueva").join("b.sql"))).is_ok());
        // El hermano que solo comparte prefijo de texto no es descendiente.
        let hermano = PathBuf::from(format!("{}-otro", base.display()));
        std::fs::create_dir_all(&hermano).unwrap();
        assert!(paths.check(text(&hermano.join("a.sql"))).is_err());
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&hermano);
    }

    #[test]
    fn el_punto_punto_no_saca_de_la_carpeta() {
        let base = dir("puntos");
        std::fs::create_dir_all(base.join("sub")).unwrap();
        let paths = AllowedPaths::default();
        paths.authorize(&base.join("sub")).unwrap();
        let afuera = format!("{}/sub/../afuera.sql", base.display());
        assert!(paths.check(&afuera).is_err());
        let inexistente = format!("{}/sub/nada/../../afuera.sql", base.display());
        assert!(paths.check(&inexistente).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn un_enlace_simbolico_hacia_afuera_no_pasa() {
        let base = dir("enlace");
        let afuera = dir("enlace-afuera");
        std::fs::write(afuera.join("secreto.txt"), "x").unwrap();
        std::os::unix::fs::symlink(&afuera, base.join("link")).unwrap();
        let paths = AllowedPaths::default();
        paths.authorize(&base).unwrap();
        assert!(paths
            .check(text(&base.join("link").join("secreto.txt")))
            .is_err());
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&afuera);
    }
}
