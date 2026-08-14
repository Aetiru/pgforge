//! Consultas guardadas con nombre.
//!
//! Distinto del historial, aunque los dos guarden SQL en SQLite: el historial es lo que pasó y se
//! borra entero sin que a nadie le duela, y esto es lo que el usuario decidió conservar. Por eso
//! tienen archivo propio cada uno —el `user_version` del esquema es del archivo, no de la tabla— y
//! por eso acá el nombre es obligatorio y único: una consulta guardada se busca por cómo se llama.
//!
//! Se guarda contra qué servidor y qué base se escribió, pero solo como dato: abrirla en otra base
//! es normal —el mismo `SELECT` contra desarrollo y contra producción es justo lo que se hace— así
//! que ni se exige que el perfil siga existiendo ni se filtra por él.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Versión del esquema del archivo. Subirla obliga a agregar el paso de migración de abajo.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS saved (
        id         INTEGER PRIMARY KEY,
        name       TEXT    NOT NULL,
        sql        TEXT    NOT NULL,
        profile_id TEXT,
        database   TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    -- `NOCASE` porque dos consultas que solo se distinguen por una mayúscula son la misma para
    -- quien las busca en una lista.
    CREATE UNIQUE INDEX IF NOT EXISTS saved_nombre ON saved (name COLLATE NOCASE);
";

/// Lo que se manda al guardar.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewQuery {
    /// `None` para una consulta nueva; con `Some` se reescribe esa.
    pub id: Option<i64>,
    pub name: String,
    pub sql: String,
    /// Contra qué servidor y qué base se escribió. Informativo: no ata la consulta a ninguno.
    pub profile_id: Option<String>,
    pub database: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: i64,
    pub name: String,
    pub sql: String,
    pub profile_id: Option<String>,
    pub database: Option<String>,
    /// Segundos desde el epoch.
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct SavedStore {
    connection: Connection,
}

const SELECT: &str =
    "SELECT id, name, sql, profile_id, database, created_at, updated_at FROM saved";

impl SavedStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;

        // Mismo criterio que el historial: que el archivo siga abriendo después de un corte importa
        // más que la última escritura.
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

    /// Todas, por nombre. Son decenas, no miles: la interfaz filtra sobre lo que ya tiene.
    pub fn list(&self) -> Result<Vec<SavedQuery>> {
        self.query(&format!("{SELECT} ORDER BY name COLLATE NOCASE"), params![])
    }

    pub fn get(&self, id: i64) -> Result<Option<SavedQuery>> {
        Ok(self
            .query(&format!("{SELECT} WHERE id = ?1"), params![id])?
            .pop())
    }

    /// Guarda una consulta nueva o reescribe una existente.
    ///
    /// El nombre repetido devuelve `Conflict` y no pisa lo que había: quien guarda con un nombre que
    /// ya existe casi siempre se olvidó de que existía, y perder la consulta anterior en silencio
    /// es exactamente lo que no puede pasar con lo único que el usuario pidió conservar.
    pub fn save(&self, input: &NewQuery, now: i64) -> Result<SavedQuery> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(Error::Config(
                "la consulta guardada necesita un nombre".to_owned(),
            ));
        }

        let taken: Option<i64> = self
            .connection
            .query_row(
                "SELECT id FROM saved WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |row| row.get(0),
            )
            .ok();

        match input.id {
            Some(id) => {
                if taken.is_some_and(|other| other != id) {
                    return Err(Error::Conflict(format!(
                        "ya hay otra consulta guardada que se llama «{name}»"
                    )));
                }

                let changed = self.connection.execute(
                    "UPDATE saved
                        SET name = ?2, sql = ?3, profile_id = ?4, database = ?5, updated_at = ?6
                      WHERE id = ?1",
                    params![id, name, input.sql, input.profile_id, input.database, now],
                )?;

                if changed == 0 {
                    // La borraron desde otra ventana entre que se abrió y se guardó.
                    return Err(Error::Conflict(
                        "la consulta guardada ya no existe".to_owned(),
                    ));
                }

                self.get(id)?
                    .ok_or_else(|| Error::History("no se pudo releer la consulta".to_owned()))
            }
            None => {
                if taken.is_some() {
                    return Err(Error::Conflict(format!(
                        "ya hay una consulta guardada que se llama «{name}»"
                    )));
                }

                self.connection.execute(
                    "INSERT INTO saved (name, sql, profile_id, database, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    params![name, input.sql, input.profile_id, input.database, now],
                )?;

                let id = self.connection.last_insert_rowid();
                self.get(id)?
                    .ok_or_else(|| Error::History("no se pudo releer la consulta".to_owned()))
            }
        }
    }

    /// Borra. Devuelve `false` si no había nada con ese identificador.
    pub fn delete(&self, id: i64) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM saved WHERE id = ?1", params![id])?
            > 0)
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<SavedQuery>> {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params, |row| {
            Ok(SavedQuery {
                id: row.get(0)?,
                name: row.get(1)?,
                sql: row.get(2)?,
                profile_id: row.get(3)?,
                database: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;

        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHORA: i64 = 1_800_000_000;

    fn store() -> SavedStore {
        // En memoria: lo que se prueba es el esquema y las consultas, no el sistema de archivos.
        SavedStore::open(":memory:").unwrap()
    }

    fn nueva(name: &str, sql: &str) -> NewQuery {
        NewQuery {
            id: None,
            name: name.into(),
            sql: sql.into(),
            profile_id: Some("servidor-1".into()),
            database: Some("app".into()),
        }
    }

    #[test]
    fn guarda_y_lista_por_nombre() {
        let store = store();
        store
            .save(&nueva("ventas del mes", "SELECT 1"), AHORA)
            .unwrap();
        store
            .save(&nueva("altas de hoy", "SELECT 2"), AHORA)
            .unwrap();

        let all = store.list().unwrap();
        let names: Vec<_> = all.iter().map(|query| query.name.as_str()).collect();
        assert_eq!(names, ["altas de hoy", "ventas del mes"]);
        assert_eq!(all[0].database.as_deref(), Some("app"));
    }

    #[test]
    fn un_nombre_repetido_no_pisa_lo_guardado() {
        let store = store();
        store.save(&nueva("ventas", "SELECT 1"), AHORA).unwrap();

        let error = store.save(&nueva("VENTAS", "SELECT 2"), AHORA).unwrap_err();
        assert!(matches!(error, Error::Conflict(_)), "{error}");

        // Y lo de antes sigue ahí, que es de lo que se trata.
        assert_eq!(store.list().unwrap()[0].sql, "SELECT 1");
    }

    #[test]
    fn reescribir_conserva_la_fecha_de_creacion() {
        let store = store();
        let first = store.save(&nueva("ventas", "SELECT 1"), AHORA).unwrap();

        let updated = store
            .save(
                &NewQuery {
                    id: Some(first.id),
                    name: "ventas por sucursal".into(),
                    sql: "SELECT 2".into(),
                    profile_id: None,
                    database: None,
                },
                AHORA + 60,
            )
            .unwrap();

        assert_eq!(updated.id, first.id);
        assert_eq!(updated.name, "ventas por sucursal");
        assert_eq!(updated.created_at, AHORA, "la creación no se toca");
        assert_eq!(updated.updated_at, AHORA + 60);
        assert_eq!(store.list().unwrap().len(), 1, "reescribe, no agrega");
    }

    #[test]
    fn renombrar_a_uno_ocupado_falla_y_a_si_misma_no() {
        let store = store();
        let ventas = store.save(&nueva("ventas", "SELECT 1"), AHORA).unwrap();
        store.save(&nueva("altas", "SELECT 2"), AHORA).unwrap();

        let mut cambio = nueva("altas", "SELECT 1");
        cambio.id = Some(ventas.id);
        assert!(matches!(
            store.save(&cambio, AHORA).unwrap_err(),
            Error::Conflict(_)
        ));

        // Guardarla con su propio nombre —solo cambiando el SQL— tiene que seguir andando.
        let mismo = NewQuery {
            id: Some(ventas.id),
            name: "ventas".into(),
            sql: "SELECT 3".into(),
            profile_id: None,
            database: None,
        };
        assert_eq!(store.save(&mismo, AHORA).unwrap().sql, "SELECT 3");
    }

    #[test]
    fn una_consulta_sin_nombre_no_se_guarda() {
        let store = store();
        let error = store.save(&nueva("   ", "SELECT 1"), AHORA).unwrap_err();
        assert!(matches!(error, Error::Config(_)), "{error}");
    }

    #[test]
    fn borrar_dice_si_habia_algo() {
        let store = store();
        let saved = store.save(&nueva("ventas", "SELECT 1"), AHORA).unwrap();

        assert!(store.delete(saved.id).unwrap());
        assert!(
            !store.delete(saved.id).unwrap(),
            "borrar dos veces no falla"
        );
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn el_nombre_se_guarda_sin_espacios_alrededor() {
        let store = store();
        let saved = store.save(&nueva("  ventas  ", "SELECT 1"), AHORA).unwrap();
        assert_eq!(saved.name, "ventas");
    }
}
