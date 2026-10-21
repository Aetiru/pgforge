//! La carpeta de migraciones: lo único del módulo que toca el disco.
//!
//! La carpeta la eligió el usuario en un diálogo nativo (el comando la verifica con `AllowedPaths`
//! antes de llegar acá), así que estas funciones reciben una ruta ya autorizada. Lo que sí
//! verifican es lo suyo: que un nombre de archivo sea un nombre y no una ruta, y que guardar nunca
//! pise una migración que ya existe.

use std::io::Write;
use std::path::Path;

use serde::Serialize;

use super::{checksum, file_name, parse_name, slug, Direction, MigrationFile};
use crate::error::{Error, Result};

/// Lo que hay en la carpeta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub files: Vec<MigrationFile>,
    /// `U…` que no tienen su `V…`: no se aplican, pero tampoco se tiran en silencio.
    pub orphan_downs: Vec<String>,
}

/// Con qué quedó guardada una migración nueva.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub version: u32,
    pub up_file: String,
    pub down_file: Option<String>,
}

fn read_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    String::from_utf8(bytes).map_err(|_| {
        Error::Config(format!(
            "{} no está en UTF-8: pgforge solo lee migraciones en ese formato",
            path.display()
        ))
    })
}

/// Lee la carpeta. Dos `V` con el mismo número es un error: no hay forma de saber cuál va primero.
pub fn scan(dir: &Path) -> Result<Scan> {
    let mut ups: Vec<(u32, String, String, String)> = Vec::new(); // versión, descripción, archivo, huella
    let mut downs: Vec<(u32, String, String)> = Vec::new(); // versión, archivo, huella

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((direction, version, description)) = parse_name(&name) else {
            continue;
        };
        let sum = checksum(&read_text(&entry.path())?);
        match direction {
            Direction::Up => ups.push((version, description, name, sum)),
            Direction::Down => downs.push((version, name, sum)),
        }
    }
    ups.sort_by_key(|(version, ..)| *version);
    downs.sort_by_key(|(version, ..)| *version);

    if let Some(pair) = ups.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::Config(format!(
            "hay dos migraciones con el número {:03}: {} y {}",
            pair[0].0, pair[0].2, pair[1].2
        )));
    }
    if let Some(pair) = downs.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::Config(format!(
            "hay dos scripts de reversa con el número {:03}: {} y {}",
            pair[0].0, pair[0].1, pair[1].1
        )));
    }

    let mut orphan_downs = Vec::new();
    for (version, name, _) in &downs {
        if !ups.iter().any(|(v, ..)| v == version) {
            orphan_downs.push(name.clone());
        }
    }

    let files = ups
        .into_iter()
        .map(|(version, description, up_file, up_checksum)| {
            let down = downs.iter().find(|(v, ..)| *v == version);
            MigrationFile {
                version,
                description,
                up_file,
                up_checksum,
                down_file: down.map(|(_, name, _)| name.clone()),
                down_checksum: down.map(|(_, _, sum)| sum.clone()),
            }
        })
        .collect();

    Ok(Scan {
        files,
        orphan_downs,
    })
}

/// El SQL de una migración, leído **ahora** del archivo.
pub fn read_sql(dir: &Path, file: &MigrationFile, direction: Direction) -> Result<String> {
    let name = match direction {
        Direction::Up => Some(&file.up_file),
        Direction::Down => file.down_file.as_ref(),
    }
    .ok_or_else(|| {
        Error::Config(format!(
            "V{:03} no tiene un script de reversa (U{:03}__…)",
            file.version, file.version
        ))
    })?;
    // Los nombres salen de `scan`, pero la regla es que ninguno se concatena a una ruta sin mirar.
    if parse_name(name).is_none() {
        return Err(Error::Config(format!("{name} no es una migración")));
    }
    read_text(&dir.join(name))
}

/// El número que sigue a los que ya hay (`V` y `U`), empezando en 1.
pub fn next_version<'a>(names: impl IntoIterator<Item = &'a str>) -> u32 {
    names
        .into_iter()
        .filter_map(parse_name)
        .map(|(_, version, _)| version)
        .max()
        .map_or(1, |max| max + 1)
}

fn write_new(path: &Path, sql: &str) -> Result<()> {
    let mut text = sql.trim_end().to_owned();
    text.push('\n');
    // `create_new` y no `create`: dos ventanas guardando a la vez no se pisan, y un archivo que ya
    // estaba ahí es una migración que quizá se aplicó.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Error::Conflict(format!("ya existe {}", path.display()))
            } else {
                Error::Io(error)
            }
        })?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

/// Guarda una migración nueva con el número siguiente.
///
/// Si el script de reversa no se puede escribir después del de ida, el de ida se borra: una
/// migración a medias (con `U` pedida y sin `U`) es peor que ninguna.
pub fn save(dir: &Path, description: &str, up_sql: &str, down_sql: Option<&str>) -> Result<Saved> {
    let slug = slug(description);
    if slug.is_empty() {
        return Err(Error::Config(
            "la descripción necesita al menos una letra o un número".to_owned(),
        ));
    }
    if up_sql.trim().is_empty() {
        return Err(Error::Config("la migración no tiene SQL".to_owned()));
    }

    let names: Vec<String> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let version = next_version(names.iter().map(String::as_str));

    let up_file = file_name(Direction::Up, version, &slug);
    write_new(&dir.join(&up_file), up_sql)?;

    let down_sql = down_sql.filter(|sql| !sql.trim().is_empty());
    let down_file = match down_sql {
        Some(sql) => {
            let name = file_name(Direction::Down, version, &slug);
            if let Err(error) = write_new(&dir.join(&name), sql) {
                let _ = std::fs::remove_file(dir.join(&up_file));
                return Err(error);
            }
            Some(name)
        }
        None => None,
    };

    Ok(Saved {
        version,
        up_file,
        down_file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pgforge-migfolder-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn numera_a_partir_de_lo_que_hay() {
        assert_eq!(next_version([]), 1);
        assert_eq!(
            next_version(["V001__a.sql", "V011__b.sql", "U012__c.sql", "notas.txt"]),
            13
        );
    }

    #[test]
    fn guarda_con_el_numero_siguiente_y_lo_lee_de_vuelta() {
        let dir = temp_dir("guardar");
        let first = save(&dir, "Crear clientes", "CREATE TABLE c (id int);", None).unwrap();
        assert_eq!(first.version, 1);
        assert_eq!(first.up_file, "V001__crear_clientes.sql");
        assert!(first.down_file.is_none());

        let second = save(
            &dir,
            "Añadir teléfono",
            "ALTER TABLE c ADD tel text;\n\n",
            Some("ALTER TABLE c DROP tel;"),
        )
        .unwrap();
        assert_eq!(second.version, 2);
        assert_eq!(
            second.down_file.as_deref(),
            Some("U002__anadir_telefono.sql")
        );

        let scan = scan(&dir).unwrap();
        assert_eq!(scan.files.len(), 2);
        assert!(scan.files[0].down_file.is_none());
        assert!(scan.files[1].down_file.is_some());
        let sql = read_sql(&dir, &scan.files[1], Direction::Up).unwrap();
        assert_eq!(sql, "ALTER TABLE c ADD tel text;\n");
        assert_eq!(scan.files[1].up_checksum, checksum(&sql));
        assert!(read_sql(&dir, &scan.files[0], Direction::Down).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rechaza_descripcion_o_sql_vacios() {
        let dir = temp_dir("vacios");
        assert!(save(&dir, "  ¿? ", "SELECT 1", None).is_err());
        assert!(save(&dir, "algo", "  \n", None).is_err());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dos_migraciones_con_el_mismo_numero_es_un_error() {
        let dir = temp_dir("duplicadas");
        std::fs::write(dir.join("V001__a.sql"), "SELECT 1").unwrap();
        std::fs::write(dir.join("V001__b.sql"), "SELECT 2").unwrap();
        let error = scan(&dir).unwrap_err();
        assert!(error.to_string().contains("001"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignora_lo_que_no_es_migracion_y_reporta_reversas_huerfanas() {
        let dir = temp_dir("huerfanas");
        std::fs::write(dir.join("V002__a.sql"), "SELECT 1").unwrap();
        std::fs::write(dir.join("U009__x.sql"), "SELECT 2").unwrap();
        std::fs::write(dir.join("README.md"), "hola").unwrap();
        std::fs::create_dir(dir.join("V003__carpeta.sql")).unwrap();
        let found = scan(&dir).unwrap();
        assert_eq!(found.files.len(), 1);
        assert_eq!(found.orphan_downs, ["U009__x.sql"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn el_checksum_no_cambia_con_los_finales_de_linea_de_windows() {
        let dir = temp_dir("crlf");
        std::fs::write(dir.join("V001__a.sql"), "SELECT 1;\r\nSELECT 2;\r\n").unwrap();
        let unix = checksum("SELECT 1;\nSELECT 2;\n");
        assert_eq!(scan(&dir).unwrap().files[0].up_checksum, unix);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
