//! Los binarios de PostgreSQL que la aplicación invoca en vez de reimplementar.
//!
//! `pg_dump` y `pg_restore` no tienen equivalente en el protocolo: el formato custom es un formato
//! propio de esas herramientas, y reimplementarlo sería un proyecto aparte. Lo que sí hace falta
//! resolver acá es encontrarlos —casi ningún instalador de Windows los deja en el `PATH`— y saber
//! su versión antes de usarlos.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;

use crate::caps::ServerVersion;
use crate::error::{Error, Result};

/// Lanza el proceso sin abrir una consola.
///
/// En Windows, un ejecutable de consola lanzado desde una aplicación gráfica se trae su propia
/// ventana negra, que aparece y desaparece sola. Con `pg_dump` eso pasaba con solo mirar el detalle
/// de una tabla —el DDL se delega en él—, así que el parpadeo era constante. `CREATE_NO_WINDOW`
/// (`0x0800_0000`) lo evita; en el resto de los sistemas no hay nada que hacer.
///
/// Va en un solo lugar y no en cada sitio que lanza un proceso: el `cfg` repetido es exactamente el
/// que se olvida al agregar la siguiente herramienta.
pub(crate) fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    PgDump,
    PgRestore,
}

impl Tool {
    fn stem(self) -> &'static str {
        match self {
            Tool::PgDump => "pg_dump",
            Tool::PgRestore => "pg_restore",
        }
    }

    fn file_name(self) -> String {
        if cfg!(windows) {
            format!("{}.exe", self.stem())
        } else {
            self.stem().to_owned()
        }
    }

    /// Variable con la que el usuario puede indicar la ruta exacta, para cuando tiene varias
    /// instalaciones y la elección automática no acierta.
    fn env_var(self) -> &'static str {
        match self {
            Tool::PgDump => "PGFORGE_PG_DUMP",
            Tool::PgRestore => "PGFORGE_PG_RESTORE",
        }
    }
}

/// Ubica el binario.
///
/// El orden importa: primero lo que el usuario configuró explícitamente, después lo que haya en el
/// `PATH`, y recién al final las rutas típicas de cada sistema, eligiendo siempre la versión más
/// alta disponible —`pg_dump` puede leer servidores más viejos que él, pero no más nuevos—.
pub fn find(tool: Tool) -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os(tool.env_var()) {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }

    let file_name = tool.file_name();

    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(&file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let roots: &[&str] = if cfg!(windows) {
        &[r"C:\Program Files\PostgreSQL"]
    } else if cfg!(target_os = "macos") {
        &["/Library/PostgreSQL", "/opt/homebrew/opt", "/usr/local/opt"]
    } else {
        &["/usr/lib/postgresql", "/usr/local/pgsql"]
    };

    let mut best: Option<(u32, PathBuf)> = None;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let candidate = entry.path().join("bin").join(&file_name);
            if !candidate.is_file() {
                continue;
            }
            // El nombre del directorio es la versión mayor en todas las distribuciones que se
            // instalan así; si no lo es, vale como último recurso.
            let version = entry
                .file_name()
                .to_string_lossy()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0);
            if best.as_ref().is_none_or(|(best, _)| version > *best) {
                best = Some((version, candidate));
            }
        }
    }

    best.map(|(_, path)| path)
}

/// Igual que [`find`], pero con el error que explica qué instalar cuando no aparece.
pub fn require(tool: Tool) -> Result<PathBuf> {
    find(tool).ok_or_else(|| {
        Error::Config(format!(
            "no se encontró {}. Instalá las herramientas cliente de PostgreSQL o indicá la ruta \
             del binario en la variable {}.",
            tool.stem(),
            tool.env_var()
        ))
    })
}

/// Versión del binario, para poder compararla con la del servidor antes de empezar.
///
/// Se saca de `--version`, que imprime algo como `pg_dump (PostgreSQL) 17.4`. El número menor puede
/// faltar (`18beta1`), así que se toma como cero: lo que importa para decidir si sirve es el mayor.
pub async fn version(path: &std::path::Path) -> Result<ServerVersion> {
    let output = hidden(&mut Command::new(path))
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| Error::Config(format!("no se pudo ejecutar {}: {e}", path.display())))?;

    let text = String::from_utf8_lossy(&output.stdout);
    parse_version(&text).ok_or_else(|| {
        Error::Config(format!(
            "no se pudo interpretar la versión de {}: {}",
            path.display(),
            text.trim()
        ))
    })
}

fn parse_version(text: &str) -> Option<ServerVersion> {
    // El primer token que empieza con un dígito, no el último: las compilaciones de Debian
    // agregan al final un `(Debian 13.23-1.pgdg13+1)` que se parece bastante a una versión.
    let number = text
        .split_whitespace()
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))?;
    let mut parts = number.split('.');

    let major: i32 = parts
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()?;
    let minor: i32 = parts
        .next()
        .and_then(|part| {
            part.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or(0);

    Some(ServerVersion::from_num(major * 10_000 + minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_la_salida_de_version() {
        assert_eq!(
            parse_version("pg_dump (PostgreSQL) 17.4\n").unwrap(),
            ServerVersion::from_num(170_004)
        );
        // La coletilla de las compilaciones de Debian no tiene que confundirse con la versión.
        assert_eq!(
            parse_version("pg_restore (PostgreSQL) 13.23 (Debian 13.23-1.pgdg13+1)\n").unwrap(),
            ServerVersion::from_num(130_023)
        );
    }

    /// Una versión sin número menor (las betas y los release candidates) vale igual: lo que decide
    /// si el binario sirve es la mayor.
    #[test]
    fn una_version_sin_menor_cuenta_como_cero() {
        assert_eq!(
            parse_version("pg_dump (PostgreSQL) 18beta1").unwrap(),
            ServerVersion::from_num(180_000)
        );
    }

    #[test]
    fn una_salida_que_no_es_una_version_no_inventa_una() {
        assert!(parse_version("").is_none());
        assert!(parse_version("command not found").is_none());
    }
}
