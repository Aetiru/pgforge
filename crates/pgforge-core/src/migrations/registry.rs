//! Registro de lo aplicado: qué migración corrió contra qué servidor y base, cuándo y con qué
//! resultado.
//!
//! Archivo propio (`migrations.db`) y no una tabla más del historial: el historial es lo que pasó y
//! se vacía entero sin que duela; este registro es lo que **decide** qué falta aplicar, y vaciarlo
//! haría reaplicar todo. Tampoco vive en la base de destino (como `flyway_schema_history`): pgforge
//! no crea objetos en servidores ajenos, y un rol de solo lectura sobre producción igual tiene que
//! poder ver qué se aplicó desde acá.
//!
//! Solo se agrega: deshacer una migración es una fila nueva con `direction = down`, no un borrado.
//! El estado de hoy lo dice la última fila de cada versión (ver [`super::status`]).

use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;

use super::Direction;
use crate::error::Result;

/// Versión del esquema del archivo. Subirla obliga a agregar el paso de migración de abajo.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS applied (
        id          INTEGER PRIMARY KEY,
        profile_id  TEXT    NOT NULL,
        database    TEXT    NOT NULL,
        server_name TEXT,
        version     INTEGER NOT NULL,
        description TEXT    NOT NULL,
        direction   TEXT    NOT NULL,
        checksum    TEXT    NOT NULL,
        applied_at  INTEGER NOT NULL,
        seconds     REAL    NOT NULL,
        succeeded   INTEGER NOT NULL,
        error       TEXT
    );
    CREATE INDEX IF NOT EXISTS applied_destino ON applied (profile_id, database, version, id);
";

/// Lo que se anota al terminar un intento.
#[derive(Debug, Clone)]
pub struct NewRecord {
    pub profile_id: String,
    pub database: String,
    /// Nombre del perfil al momento de aplicar: el perfil puede renombrarse o borrarse después, y
    /// el registro tiene que seguir diciendo contra qué se corrió.
    pub server_name: Option<String>,
    pub version: u32,
    pub description: String,
    pub direction: Direction,
    pub checksum: String,
    pub applied_at: i64,
    pub seconds: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: i64,
    pub profile_id: String,
    pub database: String,
    pub server_name: Option<String>,
    pub version: u32,
    pub description: String,
    pub direction: Direction,
    pub checksum: String,
    /// Segundos desde el epoch.
    pub applied_at: i64,
    pub seconds: f64,
    pub succeeded: bool,
    pub error: Option<String>,
}

pub struct MigrationStore {
    connection: Connection,
}

impl MigrationStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        crate::private::restrict(path.as_ref())?;
        let connection = Connection::open(path)?;
        Self::init(connection)
    }

    fn init(connection: Connection) -> Result<Self> {
        // Mismo criterio que el resto de los archivos SQLite: que siga abriendo después de un corte
        // importa más que la última fila.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < SCHEMA_VERSION {
            self.connection.execute_batch(SCHEMA)?;
            self.connection
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    pub fn record(&self, entry: &NewRecord) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO applied
                 (profile_id, database, server_name, version, description, direction, checksum,
                  applied_at, seconds, succeeded, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                entry.profile_id,
                entry.database,
                entry.server_name,
                entry.version,
                entry.description,
                entry.direction.as_str(),
                entry.checksum,
                entry.applied_at,
                entry.seconds,
                entry.error.is_none(),
                entry.error,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Todos los intentos contra un servidor y una base, del más viejo al más nuevo.
    pub fn records(&self, profile_id: &str, database: &str) -> Result<Vec<Record>> {
        let mut statement = self.connection.prepare(
            "SELECT id, profile_id, database, server_name, version, description, direction,
                    checksum, applied_at, seconds, succeeded, error
               FROM applied
              WHERE profile_id = ?1 AND database = ?2
              ORDER BY id",
        )?;
        let rows = statement.query_map(params![profile_id, database], |row| {
            Ok(Record {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                database: row.get(2)?,
                server_name: row.get(3)?,
                version: row.get(4)?,
                description: row.get(5)?,
                direction: Direction::from_str(&row.get::<_, String>(6)?),
                checksum: row.get(7)?,
                applied_at: row.get(8)?,
                seconds: row.get(9)?,
                succeeded: row.get(10)?,
                error: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        Self::init(Connection::open_in_memory().unwrap()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_record(profile: &str, database: &str, version: u32, error: Option<&str>) -> NewRecord {
        NewRecord {
            profile_id: profile.into(),
            database: database.into(),
            server_name: Some("desarrollo".into()),
            version,
            description: "crear_clientes".into(),
            direction: Direction::Up,
            checksum: "abc".into(),
            applied_at: 1_700_000_000,
            seconds: 0.25,
            error: error.map(str::to_owned),
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pgforge-migrations-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("migrations.db")
    }

    #[test]
    fn guarda_y_lista_por_servidor_y_base() {
        let store = MigrationStore::in_memory();
        store.record(&new_record("a", "app", 1, None)).unwrap();
        store
            .record(&new_record("a", "app", 2, Some("boom")))
            .unwrap();
        store.record(&new_record("a", "otra", 1, None)).unwrap();
        store.record(&new_record("b", "app", 1, None)).unwrap();

        let records = store.records("a", "app").unwrap();
        assert_eq!(records.len(), 2);
        assert!(records[0].succeeded && records[0].error.is_none());
        assert!(!records[1].succeeded);
        assert_eq!(records[1].error.as_deref(), Some("boom"));
        assert_eq!(records[0].direction, Direction::Up);
        assert!(records[0].id < records[1].id);
        assert!(store.records("c", "app").unwrap().is_empty());
    }

    #[test]
    fn el_archivo_se_reabre_con_lo_que_tenia() {
        let path = temp_path("reabrir");
        {
            let store = MigrationStore::open(&path).unwrap();
            let mut down = new_record("a", "app", 1, None);
            down.direction = Direction::Down;
            store.record(&down).unwrap();
        }
        let store = MigrationStore::open(&path).unwrap();
        let records = store.records("a", "app").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].direction, Direction::Down);
        let version: i64 = store
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
