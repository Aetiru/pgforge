//! Historial de consultas.
//!
//! En SQLite y no en un archivo suelto porque el historial se consulta, no se lee entero: buscar
//! «esa consulta que corrí la semana pasada» sobre miles de entradas es lo único que lo hace útil,
//! y para eso hace falta un índice.
//!
//! Se guarda el texto de la consulta, nunca nada del perfil más allá de su identificador: la
//! contraseña vive en el almacén de credenciales del sistema y no tiene por qué pasar por acá.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Versión del esquema del archivo. Subirla obliga a agregar el paso de migración de abajo.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS history (
        id         INTEGER PRIMARY KEY,
        profile_id TEXT    NOT NULL,
        database   TEXT    NOT NULL,
        sql        TEXT    NOT NULL,
        started_at INTEGER NOT NULL,
        seconds    REAL,
        row_count  INTEGER,
        succeeded  INTEGER NOT NULL,
        error      TEXT
    );
    CREATE INDEX IF NOT EXISTS history_reciente ON history (started_at DESC);
";

/// Lo que se registra al terminar una ejecución.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    pub profile_id: String,
    pub database: String,
    pub sql: String,
    /// Segundos desde el epoch.
    pub started_at: i64,
    pub seconds: f64,
    pub row_count: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub profile_id: String,
    pub database: String,
    pub sql: String,
    pub started_at: i64,
    pub seconds: f64,
    pub row_count: Option<i64>,
    pub succeeded: bool,
    pub error: Option<String>,
}

pub struct HistoryStore {
    connection: Connection,
}

const SELECT: &str = "SELECT id, profile_id, database, sql, started_at, seconds, row_count,
                             succeeded, error
                        FROM history";

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;

        // El historial es lo más prescindible de la aplicación: si el proceso se corta a mitad de
        // una escritura, importa mucho más que la base siga abriendo que no perder la última fila.
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

    pub fn record(&self, entry: &NewEntry) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO history
                 (profile_id, database, sql, started_at, seconds, row_count, succeeded, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.profile_id,
                entry.database,
                entry.sql,
                entry.started_at,
                entry.seconds,
                entry.row_count,
                entry.error.is_none(),
                entry.error,
            ],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    /// Lo último ejecutado, de un servidor o de todos.
    pub fn recent(&self, profile_id: Option<&str>, limit: i64) -> Result<Vec<Entry>> {
        match profile_id {
            Some(profile_id) => self.query(
                &format!("{SELECT} WHERE profile_id = ?1 ORDER BY started_at DESC LIMIT ?2"),
                params![profile_id, limit],
            ),
            None => self.query(
                &format!("{SELECT} ORDER BY started_at DESC LIMIT ?1"),
                params![limit],
            ),
        }
    }

    /// Busca por texto dentro de las consultas.
    pub fn search(&self, text: &str, limit: i64) -> Result<Vec<Entry>> {
        // `escape` para que quien busque `100%` no termine buscando cualquier cosa.
        let pattern = format!(
            "%{}%",
            text.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );

        self.query(
            &format!("{SELECT} WHERE sql LIKE ?1 ESCAPE '\\' ORDER BY started_at DESC LIMIT ?2"),
            params![pattern, limit],
        )
    }

    pub fn clear(&self) -> Result<()> {
        self.connection.execute("DELETE FROM history", [])?;
        Ok(())
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Entry>> {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params, |row| {
            Ok(Entry {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                database: row.get(2)?,
                sql: row.get(3)?,
                started_at: row.get(4)?,
                seconds: row.get(5)?,
                row_count: row.get(6)?,
                succeeded: row.get(7)?,
                error: row.get(8)?,
            })
        })?;

        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::History(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sql: &str) -> NewEntry {
        NewEntry {
            profile_id: "servidor-1".into(),
            database: "app".into(),
            sql: sql.into(),
            started_at: 1_700_000_000,
            seconds: 0.5,
            row_count: Some(3),
            error: None,
        }
    }

    fn store() -> HistoryStore {
        // En memoria: el test verifica el esquema y las consultas, no el sistema de archivos.
        HistoryStore::open(":memory:").unwrap()
    }

    #[test]
    fn guarda_y_devuelve_lo_ultimo_primero() {
        let store = store();
        store.record(&entry("SELECT 1")).unwrap();
        store
            .record(&NewEntry {
                started_at: 1_700_000_100,
                ..entry("SELECT 2")
            })
            .unwrap();

        let recientes = store.recent(None, 10).unwrap();
        assert_eq!(recientes.len(), 2);
        assert_eq!(recientes[0].sql, "SELECT 2");
        assert!(recientes[0].succeeded);
        assert_eq!(recientes[0].row_count, Some(3));
    }

    #[test]
    fn separa_por_servidor() {
        let store = store();
        store.record(&entry("SELECT 1")).unwrap();
        store
            .record(&NewEntry {
                profile_id: "servidor-2".into(),
                ..entry("SELECT 2")
            })
            .unwrap();

        assert_eq!(store.recent(Some("servidor-1"), 10).unwrap().len(), 1);
        assert_eq!(store.recent(Some("nadie"), 10).unwrap().len(), 0);
    }

    #[test]
    fn registra_las_que_fallaron() {
        let store = store();
        store
            .record(&NewEntry {
                error: Some("no existe la tabla".into()),
                row_count: None,
                ..entry("SELECT * FROM nada")
            })
            .unwrap();

        let fallida = &store.recent(None, 1).unwrap()[0];
        assert!(
            !fallida.succeeded,
            "una consulta que falló también es historia"
        );
        assert_eq!(fallida.error.as_deref(), Some("no existe la tabla"));
    }

    #[test]
    fn busca_por_texto() {
        let store = store();
        store.record(&entry("SELECT * FROM clientes")).unwrap();
        store.record(&entry("SELECT * FROM ventas")).unwrap();

        assert_eq!(store.search("clientes", 10).unwrap().len(), 1);
        assert_eq!(store.search("SELECT", 10).unwrap().len(), 2);
        assert_eq!(store.search("no está", 10).unwrap().len(), 0);
    }

    #[test]
    fn los_comodines_del_buscador_son_texto() {
        let store = store();
        store.record(&entry("SELECT 100 % 3")).unwrap();
        store.record(&entry("SELECT 1")).unwrap();

        assert_eq!(
            store.search("100 %", 10).unwrap().len(),
            1,
            "el % que escribió el usuario no puede funcionar como comodín"
        );
    }

    #[test]
    fn vaciar_lo_deja_sin_nada() {
        let store = store();
        store.record(&entry("SELECT 1")).unwrap();
        store.clear().unwrap();

        assert!(store.recent(None, 10).unwrap().is_empty());
    }

    #[test]
    fn reabrir_no_pierde_lo_guardado() {
        let archivo =
            std::env::temp_dir().join(format!("pgforge-historial-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&archivo);

        let store = HistoryStore::open(&archivo).unwrap();
        store.record(&entry("SELECT 1")).unwrap();
        drop(store);

        let reabierto = HistoryStore::open(&archivo).unwrap();
        assert_eq!(reabierto.recent(None, 10).unwrap().len(), 1);

        drop(reabierto);
        let _ = std::fs::remove_file(&archivo);
    }
}
