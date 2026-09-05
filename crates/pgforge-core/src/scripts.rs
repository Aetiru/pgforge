//! Scripts: los `.sql` de cada conexión, guardados solos, en una carpeta por conexión.
//!
//! El núcleo **recibe la raíz**, no la resuelve —mismo precedente que `conn::import::sources`—: de
//! qué directorio de configuración cuelga `scripts/` es una decisión de la aplicación de escritorio,
//! no de este crate. Lo que hay acá es lógica de verdad: sanear un nombre de conexión para que sirva
//! de carpeta, numerar scripts nuevos, recorrer el árbol, escribir sin corromper a mitad de camino, e
//! importar sin pisar lo que ya había.
//!
//! [`within`] es la guarda de seguridad: toda ruta que llega de la interfaz pasa por ahí antes de
//! escribir, renombrar o borrar. Sin eso, una ruta armada a mano borraría cualquier archivo del
//! disco.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Caracteres que Windows no permite en un nombre de archivo o carpeta. Se agrega `/` aunque no
/// moleste a Windows: en el árbol de scripts un `/` separa carpetas, y una conexión que lo tenga en
/// el nombre no puede convertirse en una sola carpeta con ese carácter adentro.
const INVALID_CHARS: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Nombres que Windows reserva para dispositivos, sin distinguir mayúsculas. Un servidor que se
/// llame igual (`CON`, típicamente por una conexión abreviada) no puede convertirse en una carpeta
/// con ese nombre tal cual: se crea, pero después nada del sistema operativo puede abrirla.
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Cuántos niveles de subcarpeta recorre [`tree`] como máximo. No es una limitación real —nadie
/// anida veinte carpetas de scripts— sino la guarda contra un enlace simbólico circular, que sin
/// tope colgaría la lectura del árbol entero.
const MAX_DEPTH: u32 = 8;

/// Sanea el nombre de una conexión para que sirva de nombre de carpeta en cualquier sistema de
/// archivos que use Windows: reemplaza los caracteres inválidos, recorta el punto o el espacio
/// final (Windows los descarta al crear la carpeta, así que dejarlos crea una carpeta con otro
/// nombre del que se ve) y evita los nombres reservados de dispositivo.
///
/// Si después de sanear no queda nada legible —una conexión que se llame solo `"..."`, por
/// ejemplo— devuelve `"_"` en vez de una cadena vacía, que ni `create_dir_all` acepta como nombre.
/// Quien llama y tiene a mano el `id` del perfil puede preferirlo como reemplazo en ese caso: acá
/// solo se garantiza no devolver la cadena vacía.
pub fn folder_name(connection: &str) -> String {
    let mut name: String = connection
        .trim()
        .chars()
        .map(|c| if INVALID_CHARS.contains(&c) { '_' } else { c })
        .collect();

    while matches!(name.chars().last(), Some('.') | Some(' ')) {
        name.pop();
    }

    if RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(&name))
    {
        name.push('_');
    }

    if name.is_empty() {
        name = "_".to_owned();
    }

    name
}

/// La carpeta de una conexión dentro de `root`, con [`folder_name`] ya aplicado. Pura: solo arma
/// la ruta, no la crea — si la carpeta todavía no existe (conexión sin scripts todavía), la ruta
/// devuelta tampoco existe, y es al llamador a quien le toca decidir si crearla o tolerarlo.
pub fn folder_path(root: &Path, connection: &str) -> PathBuf {
    root.join(folder_name(connection))
}

/// El siguiente `scriptN.sql` libre entre `existing`: rellena huecos en vez de seguir creciendo, y
/// compara por número entero, no por texto, para que `script10.sql` no confunda a `script1.sql`.
pub fn next_name(existing: &[String]) -> String {
    let used: HashSet<u32> = existing
        .iter()
        .filter_map(|name| {
            name.strip_prefix("script")?
                .strip_suffix(".sql")?
                .parse::<u32>()
                .ok()
        })
        .collect();

    let mut n = 1;
    while used.contains(&n) {
        n += 1;
    }
    format!("script{n}.sql")
}

/// Canonicaliza `path` aunque no exista todavía, y **ningún** tramo de su camino tenga por qué
/// existir: es el caso normal tanto de la raíz de scripts (no se crea hasta el primer script) como
/// del primer script de una conexión que todavía no tiene carpeta. Por eso no alcanza con
/// canonicalizar el padre inmediato —también puede faltar—: se sube por los ancestros hasta
/// encontrar el primero que sí exista, se canonicaliza ese, y se le vuelve a pegar el resto del
/// camino tal como venía escrito.
fn resolve(path: &Path) -> Result<PathBuf> {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return Ok(resolved);
    }

    let mut existing = path;
    let mut missing = Vec::new();
    loop {
        match existing.parent() {
            Some(parent) => {
                missing.push(existing.file_name().ok_or_else(|| {
                    Error::Config(format!("ruta sin nombre de archivo: {}", path.display()))
                })?);
                existing = parent;
                if existing.exists() {
                    break;
                }
            }
            None => {
                return Err(Error::Config(format!(
                    "ruta sin ningún ancestro existente: {}",
                    path.display()
                )));
            }
        }
    }

    Ok(missing
        .into_iter()
        .rev()
        .fold(std::fs::canonicalize(existing)?, |acc, part| acc.join(part)))
}

/// Exige que `path` quede adentro de `root` y devuelve la ruta canónica. Toda ruta que llega de la
/// interfaz pasa por acá antes de escribir, renombrar o borrar: es la única guarda entre un clic y
/// borrar cualquier archivo del disco.
///
/// `root` y `path` pasan los dos por [`resolve`]: la raíz de scripts tampoco existe antes del
/// primer script guardado, y exigirle que exista de antemano rompería justo ese caso normal.
pub fn within(root: &Path, path: &Path) -> Result<PathBuf> {
    let canonical_root = resolve(root)?;
    let canonical_path = resolve(path)?;

    if canonical_path.starts_with(&canonical_root) {
        Ok(canonical_path)
    } else {
        Err(Error::Config(format!(
            "la ruta {} queda fuera de la carpeta de scripts",
            path.display()
        )))
    }
}

/// Un archivo `.sql` dentro del árbol de scripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Una carpeta del árbol de scripts. En el primer nivel bajo la raíz, una por conexión —con
/// [`folder_name`] ya aplicado—; más abajo, una subcarpeta cualquiera que el usuario haya creado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFolder {
    pub name: String,
    pub path: PathBuf,
    pub files: Vec<ScriptEntry>,
    pub folders: Vec<ScriptFolder>,
}

/// Una carpeta por conexión de primer nivel bajo `root`, recorrida recursivamente. Solo entran
/// archivos `.sql`; cada lista queda ordenada alfabéticamente sin distinguir mayúsculas, para que
/// el orden no cambie según cómo se haya escrito el nombre.
///
/// Si `root` todavía no existe, no es un error: es que no se guardó ningún script todavía.
pub fn tree(root: &Path) -> Result<Vec<ScriptFolder>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut connections = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            connections.push(read_folder(&entry.path(), &name, 0)?);
        }
        // Un archivo suelto directamente bajo la raíz no pertenece a ninguna conexión: no debería
        // pasar salvo manipulación externa del directorio, y se ignora en vez de fallar.
    }

    connections.sort_by_key(|folder| folder.name.to_lowercase());
    Ok(connections)
}

fn read_folder(path: &Path, name: &str, depth: u32) -> Result<ScriptFolder> {
    let mut files = Vec::new();
    let mut folders = Vec::new();

    if depth < MAX_DEPTH {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            let entry_name = entry.file_name().to_string_lossy().into_owned();
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                folders.push(read_folder(&entry_path, &entry_name, depth + 1)?);
            } else if file_type.is_file()
                && entry_path.extension().and_then(|ext| ext.to_str()) == Some("sql")
            {
                files.push(ScriptEntry {
                    name: entry_name,
                    path: entry_path,
                });
            }
        }
    }

    folders.sort_by_key(|folder| folder.name.to_lowercase());
    files.sort_by_key(|file| file.name.to_lowercase());

    Ok(ScriptFolder {
        name: name.to_owned(),
        path: path.to_owned(),
        files,
        folders,
    })
}

pub fn read(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

/// Escribe atómico: a un archivo temporal aparte y después `rename`, como `SnippetStore::persist`.
/// El sufijo lleva el pid **y** un instante en nanosegundos, no alcanza con el pid solo: dos
/// pestañas del mismo proceso escribiendo distintos scripts a la vez comparten pid, y sin algo más
/// que las distinga sus temporales podrían pisarse entre sí antes de que cada una haga su `rename`.
pub fn write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let instant = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp = path.with_extension(format!("sql.tmp.{}.{instant}", std::process::id()));

    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn rename(old: &Path, new: &Path) -> Result<()> {
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(old, new)?;
    Ok(())
}

pub fn delete(path: &Path) -> Result<()> {
    Ok(std::fs::remove_file(path)?)
}

pub fn create_folder(path: &Path) -> Result<()> {
    Ok(std::fs::create_dir_all(path)?)
}

/// Resultado de [`import_folder`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub imported: usize,
    /// Motivo textual de lo que **falló** al copiar (permiso, etc.). Lo que se importó con otro
    /// nombre por chocar con uno existente no entra acá: eso sí se importó, solo que con otro
    /// nombre.
    pub skipped: Vec<String>,
}

/// Copia recursivamente los `.sql` de `src` hacia `dest`, conservando las subcarpetas. `dest` ya
/// tiene que ser la carpeta de una conexión (`root/<folder_name>`); esta función no la resuelve.
///
/// Una colisión de nombre no pisa el archivo existente: agrega ` (2)`, ` (3)`… hasta encontrar uno
/// libre, y ese archivo entra igual —no queda afuera del informe como si hubiera fallado—.
pub fn import_folder(src: &Path, dest: &Path) -> Result<ImportReport> {
    let mut report = ImportReport {
        imported: 0,
        skipped: Vec::new(),
    };
    import_into(src, dest, &mut report)?;
    Ok(report)
}

fn import_into(src: &Path, dest: &Path, report: &mut ImportReport) -> Result<()> {
    let entries = match std::fs::read_dir(src) {
        Ok(entries) => entries,
        Err(err) => {
            report.skipped.push(format!("{}: {err}", src.display()));
            return Ok(());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                report.skipped.push(err.to_string());
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => {
                report.skipped.push(format!("{}: {err}", path.display()));
                continue;
            }
        };

        if file_type.is_dir() {
            let sub_dest = dest.join(entry.file_name());
            if let Err(err) = std::fs::create_dir_all(&sub_dest) {
                report
                    .skipped
                    .push(format!("{}: {err}", sub_dest.display()));
                continue;
            }
            import_into(&path, &sub_dest, report)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("sql") {
            let target = match std::fs::create_dir_all(dest) {
                Ok(()) => unique_target(dest, &entry.file_name().to_string_lossy()),
                Err(err) => {
                    report.skipped.push(format!("{}: {err}", dest.display()));
                    continue;
                }
            };

            if let Err(err) = std::fs::copy(&path, &target) {
                report.skipped.push(format!("{}: {err}", path.display()));
            } else {
                report.imported += 1;
            }
        }
    }

    Ok(())
}

/// Nombre libre en `dir` para `file_name`: si ya existe, agrega ` (2)`, ` (3)`… hasta encontrar
/// uno que no choque.
fn unique_target(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut n = 2;
    loop {
        let alt = dir.join(format!("{stem} ({n}).sql"));
        if !alt.exists() {
            return alt;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Directorio temporal propio, borrado al final aunque el test panickee — mismo motivo que
    /// `teardown()` en los tests de integración contra servidores reales (documentado en el
    /// `CLAUDE.md` raíz).
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!(
                "pgforge-test-scripts-{}-{label}-{n}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn caracteres_invalidos_de_windows_se_reemplazan() {
        assert_eq!(folder_name(r#"prod\api:db"#), "prod_api_db");
        assert_eq!(folder_name("a*b?c\"d<e>f|g"), "a_b_c_d_e_f_g");
    }

    #[test]
    fn punto_y_espacio_al_final_se_recortan() {
        assert_eq!(folder_name("servidor. "), "servidor");
        assert_eq!(folder_name("servidor.. "), "servidor");
    }

    #[test]
    fn nombres_reservados_de_windows_no_se_usan_tal_cual() {
        assert_eq!(folder_name("CON"), "CON_");
        assert_eq!(folder_name("com3"), "com3_");
        assert_eq!(
            folder_name("Connections"),
            "Connections",
            "el prefijo reservado adentro de un nombre más largo no cuenta"
        );
    }

    #[test]
    fn nombre_vacio_tras_sanear_no_devuelve_cadena_vacia() {
        assert_eq!(folder_name("..."), "_");
        assert_eq!(folder_name("   "), "_");
    }

    #[test]
    fn folder_path_no_crea_la_carpeta() {
        let dir = TempDir::new("folder-path");
        let resolved = folder_path(&dir.path, "servidor-nuevo");

        assert_eq!(resolved, dir.path.join("servidor-nuevo"));
        assert!(!resolved.exists(), "resolver la ruta no debe crear nada");
    }

    #[test]
    fn numera_scripts_rellenando_huecos() {
        let existing = vec!["script1.sql".to_owned(), "script3.sql".to_owned()];
        assert_eq!(next_name(&existing), "script2.sql");
    }

    #[test]
    fn diez_no_se_confunde_con_uno() {
        let existing = vec!["script1.sql".to_owned(), "script10.sql".to_owned()];
        assert_eq!(next_name(&existing), "script2.sql");
    }

    #[test]
    fn con_todo_ocupado_sigue_al_que_sigue() {
        let existing = vec!["script1.sql".to_owned(), "script2.sql".to_owned()];
        assert_eq!(next_name(&existing), "script3.sql");
    }

    #[test]
    fn ignora_nombres_que_no_son_scripts_numerados() {
        let existing = vec!["notas.sql".to_owned(), "script.sql".to_owned()];
        assert_eq!(next_name(&existing), "script1.sql");
    }

    #[test]
    fn within_acepta_una_ruta_adentro_de_la_raiz_aunque_el_archivo_no_exista() {
        let dir = TempDir::new("within-ok");
        let target = dir.path.join("script1.sql");

        let resolved = within(&dir.path, &target).unwrap();
        assert!(resolved.starts_with(std::fs::canonicalize(&dir.path).unwrap()));
    }

    #[test]
    fn within_acepta_una_ruta_cuya_carpeta_de_conexion_tampoco_existe_todavia() {
        // El primer script de una conexión nueva: ni el archivo ni su carpeta existen aún.
        let dir = TempDir::new("within-doble-falta");
        let target = dir.path.join("servidor-nuevo").join("script1.sql");

        let resolved = within(&dir.path, &target).unwrap();
        assert!(resolved.starts_with(std::fs::canonicalize(&dir.path).unwrap()));
        assert_eq!(resolved.file_name().unwrap(), "script1.sql");
    }

    #[test]
    fn within_acepta_una_ruta_cuando_la_raiz_tampoco_existe_todavia() {
        // El primer script de toda la instalación: ni la raíz de scripts existe aún.
        let container = TempDir::new("within-raiz-falta");
        let root = container.path.join("scripts");
        let target = root.join("servidor-nuevo").join("script1.sql");

        let resolved = within(&root, &target).unwrap();
        assert!(resolved.starts_with(std::fs::canonicalize(&container.path).unwrap()));
        assert_eq!(resolved.file_name().unwrap(), "script1.sql");
    }

    #[test]
    fn within_rechaza_una_ruta_que_se_sale_de_la_raiz() {
        let dir = TempDir::new("within-out");
        let outside = std::env::temp_dir().join("pgforge-fuera-de-la-raiz.sql");

        let error = within(&dir.path, &outside).unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{error}");
    }

    #[test]
    fn escribir_y_leer_es_atomico_y_no_deja_temporales() {
        let dir = TempDir::new("write-read");
        let path = dir.path.join("script1.sql");

        write(&path, "SELECT 1;").unwrap();
        assert_eq!(read(&path).unwrap(), "SELECT 1;");

        let leftovers = std::fs::read_dir(&dir.path)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn renombra_y_borra() {
        let dir = TempDir::new("rename-delete");
        let a = dir.path.join("script1.sql");
        let b = dir.path.join("notas.sql");
        write(&a, "SELECT 1;").unwrap();

        rename(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(read(&b).unwrap(), "SELECT 1;");

        delete(&b).unwrap();
        assert!(!b.exists());
    }

    #[test]
    fn crea_carpeta_anidada() {
        let dir = TempDir::new("create-folder");
        let nested = dir.path.join("sub").join("mas");

        create_folder(&nested).unwrap();
        assert!(nested.is_dir());
    }

    #[test]
    fn arbol_vacio_si_la_raiz_no_existe() {
        let root = std::env::temp_dir().join("pgforge-scripts-raiz-inexistente-xyz");
        assert_eq!(tree(&root).unwrap(), Vec::new());
    }

    #[test]
    fn arbol_agrupa_por_conexion_ordena_y_filtra_solo_sql() {
        let dir = TempDir::new("tree");
        let conn_a = dir.path.join("servidor-a");
        let conn_b = dir.path.join("servidor-b");
        std::fs::create_dir_all(conn_a.join("sub")).unwrap();
        std::fs::create_dir_all(&conn_b).unwrap();

        write(&conn_a.join("script2.sql"), "SELECT 2;").unwrap();
        write(&conn_a.join("script1.sql"), "SELECT 1;").unwrap();
        write(&conn_a.join("nota.txt"), "no es sql").unwrap();
        write(&conn_a.join("sub").join("script3.sql"), "SELECT 3;").unwrap();

        let result = tree(&dir.path).unwrap();
        let names: Vec<_> = result.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["servidor-a", "servidor-b"]);

        let a = &result[0];
        assert_eq!(a.files.len(), 2, "el .txt no entra");
        assert_eq!(a.files[0].name, "script1.sql");
        assert_eq!(a.files[1].name, "script2.sql");
        assert_eq!(a.folders.len(), 1);
        assert_eq!(a.folders[0].name, "sub");
        assert_eq!(a.folders[0].files[0].name, "script3.sql");
    }

    #[test]
    fn importa_carpeta_con_subcarpetas_y_evita_pisar_un_nombre_repetido() {
        let dir = TempDir::new("import");
        let src = dir.path.join("origen");
        let dest = dir.path.join("destino");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::create_dir_all(&dest).unwrap();

        std::fs::write(src.join("a.sql"), "SELECT 1;").unwrap();
        std::fs::write(src.join("sub").join("b.sql"), "SELECT 2;").unwrap();
        std::fs::write(src.join("nota.txt"), "no es sql").unwrap();
        // Ya hay un "a.sql" en destino: la importación no lo puede pisar.
        std::fs::write(dest.join("a.sql"), "original").unwrap();

        let report = import_folder(&src, &dest).unwrap();
        assert_eq!(report.imported, 2, "a.sql y sub/b.sql; el .txt no cuenta");
        assert!(report.skipped.is_empty());

        assert_eq!(
            std::fs::read_to_string(dest.join("a.sql")).unwrap(),
            "original",
            "no se pisó lo que ya había"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("a (2).sql")).unwrap(),
            "SELECT 1;",
            "la colisión entró igual, con otro nombre"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("sub").join("b.sql")).unwrap(),
            "SELECT 2;"
        );
    }
}
