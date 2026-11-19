//! Versiones anteriores del texto de una pestaña.
//!
//! Deshacer del editor se pierde al cerrar la pestaña, y es justo entonces cuando se la necesita
//! («borré la consulta de ayer y ya no está»). Acá se guardan copias del texto, por pestaña, cada
//! vez que se ejecuta y cada tanto mientras se escribe; el momento de guardar lo decide la
//! interfaz, este módulo solo sabe **qué conservar**.
//!
//! Archivo aparte (`versions.db`), como el historial y las consultas guardadas: el `user_version` es
//! del archivo, y esto se puede borrar entero sin perder nada que alguien haya decidido conservar.
//! Los topes existen para que la tabla no crezca sin límite en una sesión larga: 50 copias por
//! pestaña, 60 días y 2 MB por copia —un script de 2 MB es un volcado pegado, no algo que se quiera
//! recuperar cada rato—.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS versions (
        id   INTEGER PRIMARY KEY,
        key  TEXT    NOT NULL,
        at   INTEGER NOT NULL,
        text TEXT    NOT NULL
    );
    CREATE INDEX IF NOT EXISTS versions_por_clave ON versions (key, at DESC, id DESC);
";

/// Copias que se guardan por pestaña.
pub const MAX_PER_KEY: i64 = 50;
/// Cuánto se conserva una copia.
pub const MAX_AGE_SECONDS: i64 = 60 * 86_400;
/// Más que esto no se guarda.
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
/// Cuánto del texto viaja como vista previa en la lista.
const PREVIEW_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub id: i64,
    /// Segundos desde el epoch.
    pub at: i64,
    /// Largo del texto, en caracteres.
    pub size: i64,
    /// El principio del texto en una sola línea, para reconocer la versión en una lista.
    pub preview: String,
}

/// Los primeros caracteres, con todo el espacio en blanco colapsado a uno solo.
pub fn preview_of(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > PREVIEW_CHARS {
        let cut: String = flat.chars().take(PREVIEW_CHARS).collect();
        format!("{cut}…")
    } else {
        flat
    }
}

pub struct VersionStore {
    connection: Connection,
}

impl VersionStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        crate::private::restrict(path.as_ref())?;
        let connection = Connection::open(path)?;
        // Mismo criterio que el historial: que el archivo siga abriendo tras un corte importa más
        // que la última escritura.
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

    /// Guarda una copia y devuelve su id, o `None` si no hacía falta: el texto es idéntico a la
    /// última copia de esa pestaña, está vacío o pasa de [`MAX_BYTES`]. Ninguno de esos casos es un
    /// error: guardar de más es lo normal en un guardado automático.
    pub fn save(&self, key: &str, text: &str, now: i64) -> Result<Option<i64>> {
        if text.trim().is_empty() || text.len() > MAX_BYTES {
            return Ok(None);
        }
        let last: Option<String> = self
            .connection
            .query_row(
                "SELECT text FROM versions WHERE key = ?1 ORDER BY at DESC, id DESC LIMIT 1",
                params![key],
                |row| row.get(0),
            )
            .ok();
        if last.as_deref() == Some(text) {
            return Ok(None);
        }

        self.connection.execute(
            "INSERT INTO versions (key, at, text) VALUES (?1, ?2, ?3)",
            params![key, now, text],
        )?;
        let id = self.connection.last_insert_rowid();

        // Lo viejo se poda al guardar: no hace falta un proceso aparte para mantener los topes.
        self.connection.execute(
            "DELETE FROM versions
              WHERE key = ?1
                AND id NOT IN (SELECT id FROM versions WHERE key = ?1
                                ORDER BY at DESC, id DESC LIMIT ?2)",
            params![key, MAX_PER_KEY],
        )?;
        self.connection.execute(
            "DELETE FROM versions WHERE at < ?1",
            params![now - MAX_AGE_SECONDS],
        )?;
        Ok(Some(id))
    }

    /// Las copias de una pestaña, la más nueva primero.
    pub fn list(&self, key: &str) -> Result<Vec<VersionMeta>> {
        // Se trae un pedazo de más del texto: el recorte a 120 caracteres se hace acá, no en SQL,
        // porque colapsar el espacio en blanco antes de recortar es lo que da una vista previa útil.
        let mut statement = self.connection.prepare(
            "SELECT id, at, length(text), substr(text, 1, 600)
               FROM versions WHERE key = ?1 ORDER BY at DESC, id DESC",
        )?;
        let rows = statement.query_map(params![key], |row| {
            let head: String = row.get(3)?;
            Ok(VersionMeta {
                id: row.get(0)?,
                at: row.get(1)?,
                size: row.get(2)?,
                preview: preview_of(&head),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT text FROM versions WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .ok())
    }

    pub fn clear(&self, key: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM versions WHERE key = ?1", params![key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> VersionStore {
        VersionStore::open(":memory:").unwrap()
    }

    #[test]
    fn guarda_lista_y_devuelve_la_mas_nueva_primero() {
        let store = store();
        store.save("t1", "SELECT 1", 100).unwrap().unwrap();
        let segunda = store.save("t1", "SELECT 2", 200).unwrap().unwrap();
        let lista = store.list("t1").unwrap();
        assert_eq!(lista.len(), 2);
        assert_eq!(lista[0].id, segunda);
        assert_eq!(lista[0].preview, "SELECT 2");
        assert_eq!(store.get(segunda).unwrap().as_deref(), Some("SELECT 2"));
        assert!(store.list("otra").unwrap().is_empty());
    }

    #[test]
    fn no_repite_la_ultima_copia_pero_si_una_anterior() {
        let store = store();
        assert!(store.save("t", "A", 1).unwrap().is_some());
        assert!(store.save("t", "A", 2).unwrap().is_none());
        assert!(store.save("t", "B", 3).unwrap().is_some());
        // Volver a A es un cambio respecto de la última (B): se guarda.
        assert!(store.save("t", "A", 4).unwrap().is_some());
        assert_eq!(store.list("t").unwrap().len(), 3);
    }

    #[test]
    fn rechaza_vacio_y_lo_demasiado_grande_sin_error() {
        let store = store();
        assert!(store.save("t", "  \n ", 1).unwrap().is_none());
        let enorme = "x".repeat(MAX_BYTES + 1);
        assert!(store.save("t", &enorme, 1).unwrap().is_none());
        assert!(store.list("t").unwrap().is_empty());
    }

    #[test]
    fn conserva_solo_las_ultimas_cincuenta_por_pestana() {
        let store = store();
        for n in 0..60 {
            store.save("t", &format!("SELECT {n}"), 1_000 + n).unwrap();
        }
        store.save("otra", "SELECT 0", 1_000).unwrap();
        let lista = store.list("t").unwrap();
        assert_eq!(lista.len() as i64, MAX_PER_KEY);
        assert_eq!(lista[0].preview, "SELECT 59");
        assert_eq!(lista.last().unwrap().preview, "SELECT 10");
        assert_eq!(store.list("otra").unwrap().len(), 1);
    }

    #[test]
    fn descarta_lo_de_mas_de_sesenta_dias() {
        let store = store();
        store.save("t", "viejo", 0).unwrap();
        store.save("t", "nuevo", MAX_AGE_SECONDS + 10).unwrap();
        let lista = store.list("t").unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].preview, "nuevo");
    }

    #[test]
    fn la_vista_previa_es_una_linea_de_ciento_veinte_caracteres() {
        assert_eq!(
            preview_of("SELECT\n    a,\n    b\nFROM t"),
            "SELECT a, b FROM t"
        );
        let larga = "a ".repeat(200);
        let preview = preview_of(&larga);
        assert_eq!(preview.chars().count(), PREVIEW_CHARS + 1);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn clear_borra_solo_esa_pestana() {
        let store = store();
        store.save("a", "x", 1).unwrap();
        store.save("b", "y", 1).unwrap();
        store.clear("a").unwrap();
        assert!(store.list("a").unwrap().is_empty());
        assert_eq!(store.list("b").unwrap().len(), 1);
    }
}
