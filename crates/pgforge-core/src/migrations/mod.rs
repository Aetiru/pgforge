//! Migraciones versionadas: el script de «comparar esquemas» (o cualquier otro) guardado como
//! archivo numerado, y el registro de qué se aplicó contra cada servidor.
//!
//! Tres piezas, como en `compare`: [`folder`] es lo único que toca el disco, [`registry`] es lo
//! único que toca SQLite, y este archivo es **puro** —nombres, numeración, checksum, estado y orden
//! de aplicación—, que es donde vive el riesgo: aplicar una migración fuera de orden, o dar por
//! buena una que cambió en disco después de correrse, se ve leyendo la lógica, no conectando.
//!
//! Convención de archivos (la de Flyway, para que la carpeta sirva en otras herramientas):
//! `V012__crear_clientes.sql` sube y `U012__crear_clientes.sql` —opcional— deshace.

pub mod folder;
pub mod registry;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::sql::lex::{lex, TokenKind};
use crate::sql::split::split;

pub use registry::{MigrationStore, NewRecord, Record};

/// Hacia dónde va una migración.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// `V…`: aplicar.
    Up,
    /// `U…`: deshacer.
    Down,
}

impl Direction {
    pub fn prefix(self) -> char {
        match self {
            Direction::Up => 'V',
            Direction::Down => 'U',
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    pub(crate) fn from_str(text: &str) -> Self {
        if text == "down" {
            Direction::Down
        } else {
            Direction::Up
        }
    }
}

/// Largo máximo de la descripción en el nombre de archivo. Hay sistemas de archivos con techos
/// cortos y un nombre de cien caracteres no se lee en ninguna lista.
const MAX_SLUG: usize = 60;

/// Convierte una descripción libre en el tramo que va en el nombre del archivo.
///
/// Minúsculas, sin tildes ni `ñ`, todo lo que no sea letra o dígito pasa a `_`. Se hace acá y no se
/// deja pasar tal cual porque el nombre termina en un sistema de archivos ajeno (Windows no admite
/// `:` ni `?`) y en un control de versiones que no quiere espacios.
pub fn slug(description: &str) -> String {
    let mut out = String::new();
    for ch in description.chars() {
        let plain = match ch {
            'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            c => c,
        };
        if plain.is_ascii_alphanumeric() {
            out.push(plain.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
        if out.len() >= MAX_SLUG {
            break;
        }
    }
    out.trim_end_matches('_').to_owned()
}

/// El nombre de archivo de una migración: `V012__crear_clientes.sql`.
///
/// El número se rellena a tres cifras para que el orden alfabético de la carpeta sea el orden de
/// aplicación hasta la 999; pasado eso crece solo y el orden numérico —el que usa [`parse_name`]—
/// sigue siendo el correcto.
pub fn file_name(direction: Direction, version: u32, slug: &str) -> String {
    format!("{}{:03}__{}.sql", direction.prefix(), version, slug)
}

/// Lo que dice el nombre de un archivo, o `None` si no es una migración.
pub fn parse_name(name: &str) -> Option<(Direction, u32, String)> {
    let stem = name.strip_suffix(".sql")?;
    let mut chars = stem.chars();
    let direction = match chars.next()? {
        'V' => Direction::Up,
        'U' => Direction::Down,
        _ => return None,
    };
    let (number, description) = chars.as_str().split_once("__")?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) || description.is_empty() {
        return None;
    }
    Some((direction, number.parse().ok()?, description.to_owned()))
}

/// Huella del SQL de una migración.
///
/// Se normaliza antes de calcularla —sin BOM, con `\n` en vez de `\r\n`—: el mismo archivo que pasa
/// por Git en Windows cambia de finales de línea, y eso no puede contar como «la migración cambió».
pub fn checksum(sql: &str) -> String {
    let normalized = sql.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let digest = Sha256::digest(normalized.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Una migración tal como está en la carpeta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationFile {
    pub version: u32,
    /// El tramo del nombre, con `_`.
    pub description: String,
    pub up_file: String,
    pub up_checksum: String,
    pub down_file: Option<String>,
    pub down_checksum: Option<String>,
}

/// Cómo está una migración respecto del servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// No se aplicó.
    Pending,
    /// Se aplicó y el archivo sigue igual.
    Applied,
    /// Se aplicó, pero el archivo cambió desde entonces: lo que está en el servidor ya no es lo que
    /// dice la carpeta, y seguir como si nada escondería la diferencia.
    Changed,
    /// El último intento terminó con error.
    Failed,
}

/// Una fila del estado: la migración de la carpeta y lo que el registro sabe de ella.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub version: u32,
    pub description: String,
    pub state: State,
    pub checksum: String,
    /// La huella con la que se aplicó, si se aplicó.
    pub applied_checksum: Option<String>,
    pub applied_at: Option<i64>,
    pub has_down: bool,
    pub error: Option<String>,
    /// Hay una aplicada con número mayor: correrla ahora es meterla en el medio de la historia.
    pub out_of_order: bool,
}

/// Una migración que el registro dice aplicada pero que ya no está en la carpeta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Missing {
    pub version: u32,
    pub description: String,
    pub applied_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub items: Vec<Item>,
    pub missing: Vec<Missing>,
}

/// Cruza la carpeta con el registro de un servidor y una base.
///
/// Manda el **último** registro de cada versión: aplicar, deshacer y volver a aplicar deja tres
/// filas, y solo la última dice cómo está hoy. `records` puede venir en cualquier orden.
pub fn status(files: &[MigrationFile], records: &[Record]) -> Report {
    let latest = |version: u32| {
        records
            .iter()
            .filter(|record| record.version == version)
            .max_by_key(|record| record.id)
    };
    let is_applied = |record: &Record| record.succeeded && record.direction == Direction::Up;

    let max_applied = records
        .iter()
        .map(|record| record.version)
        .filter(|&version| latest(version).is_some_and(is_applied))
        .max();

    let mut items: Vec<Item> = files
        .iter()
        .map(|file| {
            let (state, applied_checksum, applied_at, error) = match latest(file.version) {
                None => (State::Pending, None, None, None),
                Some(record) if !record.succeeded => (
                    State::Failed,
                    None,
                    None,
                    Some(record.error.clone().unwrap_or_default()),
                ),
                // Se deshizo: vuelve a estar pendiente.
                Some(record) if record.direction == Direction::Down => {
                    (State::Pending, None, None, None)
                }
                Some(record) => {
                    let state = if record.checksum == file.up_checksum {
                        State::Applied
                    } else {
                        State::Changed
                    };
                    (
                        state,
                        Some(record.checksum.clone()),
                        Some(record.applied_at),
                        None,
                    )
                }
            };
            Item {
                version: file.version,
                description: file.description.clone(),
                state,
                checksum: file.up_checksum.clone(),
                applied_checksum,
                applied_at,
                has_down: file.down_file.is_some(),
                error,
                out_of_order: matches!(state, State::Pending | State::Failed)
                    && max_applied.is_some_and(|max| file.version < max),
            }
        })
        .collect();
    items.sort_by_key(|item| item.version);

    let mut missing: Vec<Missing> = Vec::new();
    let mut seen: Vec<u32> = Vec::new();
    for record in records {
        if seen.contains(&record.version) {
            continue;
        }
        seen.push(record.version);
        if files.iter().any(|file| file.version == record.version) {
            continue;
        }
        if let Some(last) = latest(record.version).filter(|last| is_applied(last)) {
            missing.push(Missing {
                version: last.version,
                description: last.description.clone(),
                applied_at: last.applied_at,
            });
        }
    }
    missing.sort_by_key(|item| item.version);

    Report { items, missing }
}

impl Report {
    /// Las que faltan aplicar, en el orden en que se pueden correr.
    pub fn pending(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|item| matches!(item.state, State::Pending | State::Failed))
            .collect()
    }

    pub fn item(&self, version: u32) -> Result<&Item> {
        self.items
            .iter()
            .find(|item| item.version == version)
            .ok_or_else(|| Error::Config(format!("no hay una migración V{version:03}")))
    }

    /// Verifica que `version` se pueda correr hacia `direction` **ahora**, o dice por qué no.
    ///
    /// Subir exige que ninguna aplicada haya cambiado y que no quede una pendiente anterior (salvo
    /// reintentar la misma que falló): aplicar la 7 con la 6 sin correr deja un esquema que nadie
    /// probó. Bajar solo deshace la **última** aplicada, de a una, y pide que exista su `U…`.
    pub fn check(&self, version: u32, direction: Direction) -> Result<()> {
        let item = self.item(version)?;

        match direction {
            Direction::Up => {
                if let Some(changed) = self.items.iter().find(|i| i.state == State::Changed) {
                    return Err(Error::Conflict(format!(
                        "V{:03} se aplicó con otro contenido del que tiene hoy el archivo: \
                         resolverlo antes de aplicar más migraciones",
                        changed.version
                    )));
                }
                if item.state == State::Applied {
                    return Err(Error::Conflict(format!(
                        "V{version:03} ya está aplicada en este servidor"
                    )));
                }
                if let Some(before) = self
                    .pending()
                    .into_iter()
                    .find(|other| other.version < version)
                {
                    return Err(Error::Conflict(format!(
                        "antes de V{version:03} falta aplicar V{:03}",
                        before.version
                    )));
                }
                Ok(())
            }
            Direction::Down => {
                if !matches!(item.state, State::Applied | State::Changed) {
                    return Err(Error::Conflict(format!(
                        "V{version:03} no está aplicada: no hay nada que deshacer"
                    )));
                }
                if !item.has_down {
                    return Err(Error::Config(format!(
                        "V{version:03} no tiene un script de reversa (U{version:03}__…)"
                    )));
                }
                if let Some(later) = self.items.iter().find(|other| {
                    other.version > version && matches!(other.state, State::Applied | State::Changed)
                }) {
                    return Err(Error::Conflict(format!(
                        "V{:03} se aplicó después: deshacer de a una, empezando por la última",
                        later.version
                    )));
                }
                Ok(())
            }
        }
    }
}

/// Cómo se ejecuta el SQL de una migración.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Todo en una transacción: o queda aplicada entera o no queda nada.
    Atomic,
    /// El script trae su propio `BEGIN`: se manda tal cual, y no se le pone otro encima.
    SelfManaged,
    /// Una sentencia por vez y sin transacción, porque alguna no puede correr adentro de una
    /// (`CREATE INDEX CONCURRENTLY`, `VACUUM`, …). Si falla a la mitad, lo anterior queda hecho.
    Sequential,
}

/// Lo que efectivamente se le manda al servidor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
    pub mode: Mode,
    /// Un elemento por viaje al servidor.
    pub steps: Vec<String>,
}

impl Execution {
    /// El texto para la vista previa: lo mismo que se va a ejecutar, no una descripción.
    pub fn text(&self) -> String {
        self.steps.iter().map(|s| format!("{s};\n")).collect()
    }
}

/// Las palabras de una sentencia en mayúsculas, sin comentarios ni literales.
fn words(statement: &str) -> Vec<String> {
    lex(statement)
        .into_iter()
        .filter(|token| token.kind == TokenKind::Word)
        .map(|token| token.text.to_ascii_uppercase())
        .collect()
}

/// Sentencias que PostgreSQL rechaza adentro de un bloque transaccional.
fn needs_no_transaction(words: &[String]) -> bool {
    let first = words.first().map(String::as_str);
    let second = words.get(1).map(String::as_str);
    words.iter().any(|word| word == "CONCURRENTLY")
        || matches!(first, Some("VACUUM" | "CLUSTER"))
        || matches!(
            (first, second),
            (Some("CREATE" | "DROP"), Some("DATABASE" | "TABLESPACE"))
                | (Some("ALTER"), Some("SYSTEM"))
        )
}

/// Decide cómo correr `sql`. Pura: es lo que muestra la vista previa y lo que ejecuta la tarea.
pub fn execution(sql: &str) -> Execution {
    let texts: Vec<String> = split(sql).into_iter().map(|s| s.text).collect();
    let words: Vec<Vec<String>> = texts.iter().map(|text| words(text)).collect();

    if words
        .first()
        .and_then(|first| first.first())
        .is_some_and(|word| word == "BEGIN" || word == "START")
    {
        return Execution {
            mode: Mode::SelfManaged,
            steps: vec![texts.join(";\n")],
        };
    }
    if words.iter().any(|w| needs_no_transaction(w)) {
        return Execution {
            mode: Mode::Sequential,
            steps: texts,
        };
    }
    if texts.is_empty() {
        return Execution {
            mode: Mode::Atomic,
            steps: Vec::new(),
        };
    }
    let mut steps = vec!["BEGIN".to_owned()];
    steps.extend(texts);
    steps.push("COMMIT".to_owned());
    Execution {
        mode: Mode::Atomic,
        steps: vec![steps.join(";\n")],
    }
}

/// Una migración lista para mostrar y aplicar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub version: u32,
    pub description: String,
    pub direction: Direction,
    /// La huella del archivo en este momento. La interfaz la devuelve al aplicar, y si el archivo
    /// cambió entre la vista previa y el «Aplicar», no se ejecuta: se aplica lo que se vio.
    pub checksum: String,
    pub execution: Execution,
    pub warnings: Vec<String>,
}

/// Arma el plan de `sql` ya leído del archivo. Los avisos son lo que conviene saber antes de
/// confirmar, no motivos de rechazo (esos los da [`Report::check`]).
pub fn plan(item: &Item, direction: Direction, sql: &str, report: &Report) -> Plan {
    let execution = execution(sql);
    let mut warnings = Vec::new();
    if execution.steps.is_empty() {
        warnings.push("El archivo no tiene ninguna sentencia.".to_owned());
    }
    if execution.mode == Mode::Sequential {
        warnings.push(
            "Incluye sentencias que no pueden correr dentro de una transacción: se ejecutan una \
             por una, y si alguna falla las anteriores quedan hechas."
                .to_owned(),
        );
    }
    if direction == Direction::Up && item.out_of_order {
        warnings.push(format!(
            "Hay migraciones con número mayor ya aplicadas: V{:03} entra fuera de orden.",
            item.version
        ));
    }
    if direction == Direction::Up && item.state == State::Failed {
        warnings.push(
            "El intento anterior falló: si corrió sin transacción, parte pudo haber quedado hecha."
                .to_owned(),
        );
    }
    if !report.missing.is_empty() {
        warnings.push(format!(
            "El servidor tiene {} migración(es) aplicada(s) que ya no están en la carpeta.",
            report.missing.len()
        ));
    }
    Plan {
        version: item.version,
        description: item.description.clone(),
        direction,
        checksum: checksum(sql),
        execution,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(version: u32, sql: &str, down: bool) -> MigrationFile {
        MigrationFile {
            version,
            description: format!("m{version}"),
            up_file: file_name(Direction::Up, version, &format!("m{version}")),
            up_checksum: checksum(sql),
            down_file: down.then(|| file_name(Direction::Down, version, "x")),
            down_checksum: None,
        }
    }

    fn record(id: i64, version: u32, direction: Direction, sql: &str, ok: bool) -> Record {
        Record {
            id,
            profile_id: "p".into(),
            database: "app".into(),
            server_name: None,
            version,
            description: format!("m{version}"),
            direction,
            checksum: checksum(sql),
            applied_at: 1_700_000_000 + id,
            seconds: 0.1,
            succeeded: ok,
            error: (!ok).then(|| "boom".to_owned()),
        }
    }

    #[test]
    fn el_slug_quita_tildes_y_simbolos() {
        assert_eq!(
            slug("Añadir columna «teléfono» a clientes"),
            "anadir_columna_telefono_a_clientes"
        );
        assert_eq!(slug("  --  "), "");
        assert!(slug(&"a".repeat(200)).len() <= MAX_SLUG);
    }

    #[test]
    fn el_nombre_se_arma_y_se_lee_de_vuelta() {
        let name = file_name(Direction::Up, 12, "crear_clientes");
        assert_eq!(name, "V012__crear_clientes.sql");
        assert_eq!(
            parse_name(&name),
            Some((Direction::Up, 12, "crear_clientes".to_owned()))
        );
        assert_eq!(
            parse_name("U1234__x.sql"),
            Some((Direction::Down, 1234, "x".to_owned()))
        );
    }

    #[test]
    fn rechaza_nombres_que_no_son_migraciones() {
        for name in [
            "V__x.sql",
            "V1_x.sql",
            "Vx1__y.sql",
            "V001__.sql",
            "V001__x.txt",
            "R001__x.sql",
            "README.md",
        ] {
            assert_eq!(parse_name(name), None, "{name}");
        }
    }

    #[test]
    fn el_checksum_ignora_finales_de_linea_y_bom() {
        assert_eq!(checksum("a\r\nb\r\n"), checksum("a\nb\n"));
        assert_eq!(checksum("\u{feff}a"), checksum("a"));
        assert_ne!(checksum("a"), checksum("b"));
        assert_eq!(checksum("").len(), 64);
    }

    #[test]
    fn distingue_pendiente_aplicada_y_cambiada() {
        let files = [file(1, "A", false), file(2, "B2", false), file(3, "C", false)];
        let records = [
            record(1, 1, Direction::Up, "A", true),
            record(2, 2, Direction::Up, "B", true),
        ];
        let report = status(&files, &records);
        let states: Vec<_> = report.items.iter().map(|i| i.state).collect();
        assert_eq!(states, [State::Applied, State::Changed, State::Pending]);
        assert_eq!(report.pending().len(), 1);
    }

    #[test]
    fn manda_el_ultimo_registro_de_cada_version() {
        let files = [file(1, "A", true)];
        let records = [
            record(1, 1, Direction::Up, "A", true),
            record(2, 1, Direction::Down, "A", true),
        ];
        assert_eq!(status(&files, &records).items[0].state, State::Pending);

        let records = [
            record(1, 1, Direction::Up, "A", true),
            record(2, 1, Direction::Down, "A", true),
            record(3, 1, Direction::Up, "A", false),
        ];
        let report = status(&files, &records);
        assert_eq!(report.items[0].state, State::Failed);
        assert_eq!(report.items[0].error.as_deref(), Some("boom"));
    }

    #[test]
    fn avisa_de_la_aplicada_que_ya_no_esta_en_la_carpeta() {
        let records = [record(1, 5, Direction::Up, "X", true)];
        let report = status(&[file(1, "A", false)], &records);
        assert_eq!(report.missing.len(), 1);
        assert_eq!(report.missing[0].version, 5);
    }

    #[test]
    fn solo_se_aplica_en_orden() {
        let files = [file(1, "A", false), file(2, "B", false), file(3, "C", false)];
        let report = status(&files, &[]);
        assert!(report.check(1, Direction::Up).is_ok());
        assert!(matches!(
            report.check(2, Direction::Up),
            Err(Error::Conflict(_))
        ));

        let records = [record(1, 1, Direction::Up, "A", true)];
        let report = status(&files, &records);
        assert!(matches!(
            report.check(1, Direction::Up),
            Err(Error::Conflict(_))
        ));
        assert!(report.check(2, Direction::Up).is_ok());
        assert!(report.check(3, Direction::Up).is_err());
    }

    #[test]
    fn reintentar_la_que_fallo_se_permite() {
        let files = [file(1, "A", false), file(2, "B", false)];
        let records = [
            record(1, 1, Direction::Up, "A", true),
            record(2, 2, Direction::Up, "B", false),
        ];
        assert!(status(&files, &records).check(2, Direction::Up).is_ok());
    }

    #[test]
    fn una_aplicada_modificada_frena_las_siguientes() {
        let files = [file(1, "A2", false), file(2, "B", false)];
        let records = [record(1, 1, Direction::Up, "A", true)];
        let error = status(&files, &records)
            .check(2, Direction::Up)
            .unwrap_err();
        assert!(error.to_string().contains("V001"));
    }

    #[test]
    fn marca_la_pendiente_que_quedo_en_el_medio() {
        let files = [file(1, "A", false), file(2, "B", false)];
        let records = [record(1, 2, Direction::Up, "B", true)];
        let report = status(&files, &records);
        assert!(report.items[0].out_of_order);
        assert!(!report.items[1].out_of_order);
    }

    #[test]
    fn deshacer_pide_reversa_y_va_de_a_una_desde_la_ultima() {
        let files = [file(1, "A", true), file(2, "B", true), file(3, "C", false)];
        let records = [
            record(1, 1, Direction::Up, "A", true),
            record(2, 2, Direction::Up, "B", true),
            record(3, 3, Direction::Up, "C", true),
        ];
        let report = status(&files, &records);
        assert!(matches!(
            report.check(3, Direction::Down),
            Err(Error::Config(_))
        ));
        assert!(matches!(
            report.check(2, Direction::Down),
            Err(Error::Conflict(_))
        ));

        let report = status(&files[..2], &records[..2]);
        assert!(report.check(2, Direction::Down).is_ok());
        assert!(report.check(1, Direction::Down).is_err());
        assert!(status(&files, &[]).check(1, Direction::Down).is_err());
    }

    #[test]
    fn lo_transaccional_va_envuelto_en_begin_y_commit() {
        let run = execution("CREATE TABLE a (id int); ALTER TABLE a ADD b int;");
        assert_eq!(run.mode, Mode::Atomic);
        assert_eq!(run.steps.len(), 1);
        assert!(run.steps[0].starts_with("BEGIN;\n"));
        assert!(run.steps[0].ends_with("COMMIT"));
        assert!(run.text().ends_with("COMMIT;\n"));
    }

    #[test]
    fn lo_que_no_admite_transaccion_corre_sentencia_por_sentencia() {
        let run = execution("CREATE TABLE a (id int); CREATE INDEX CONCURRENTLY i ON a (id);");
        assert_eq!(run.mode, Mode::Sequential);
        assert_eq!(run.steps.len(), 2);
        assert_eq!(execution("vacuum a").mode, Mode::Sequential);
        assert_eq!(execution("CREATE DATABASE x").mode, Mode::Sequential);
    }

    #[test]
    fn concurrently_dentro_de_un_literal_no_cuenta() {
        let run = execution("INSERT INTO t VALUES ('create index concurrently'); -- vacuum");
        assert_eq!(run.mode, Mode::Atomic);
    }

    #[test]
    fn un_script_con_su_propio_begin_no_se_envuelve() {
        let run = execution("-- cabecera\nBEGIN; CREATE TABLE a (id int); COMMIT;");
        assert_eq!(run.mode, Mode::SelfManaged);
        assert_eq!(run.steps.len(), 1);
        assert!(!run.steps[0].starts_with("BEGIN;\nBEGIN"));
    }

    #[test]
    fn un_archivo_vacio_no_arma_una_transaccion_vacia() {
        let run = execution("-- nada\n");
        assert!(run.steps.is_empty());
    }

    #[test]
    fn el_plan_avisa_de_lo_que_hay_que_saber() {
        let files = [file(1, "A", false), file(2, "B", false)];
        let records = [record(1, 2, Direction::Up, "B", true)];
        let report = status(&files, &records);
        let sql = "CREATE INDEX CONCURRENTLY i ON a (id)";
        let plan = plan(&report.items[0], Direction::Up, sql, &report);
        assert_eq!(plan.warnings.len(), 2);
        assert_eq!(plan.checksum, checksum(sql));
    }
}
