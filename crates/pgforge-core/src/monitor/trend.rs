//! Muestras guardadas para proyectar hacia adelante.
//!
//! El servidor solo conoce el tamaño de hoy: «cuánto crece por semana» y «cuándo se agota esta
//! secuencia» necesitan haberlo mirado antes. Cada vez que se abre el mapa de espacio o la lista de
//! secuencias se anota una muestra, como mucho una por hora, en `trends.db`.
//!
//! Archivo aparte, como el historial y el registro de migraciones: el `user_version` del esquema es
//! del archivo, y esto se puede borrar entero sin que nada más pierda datos —solo se reinicia el
//! cálculo—.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS samples (
        server   TEXT    NOT NULL,
        database TEXT    NOT NULL,
        kind     TEXT    NOT NULL,
        name     TEXT    NOT NULL,
        at       INTEGER NOT NULL,
        value    REAL    NOT NULL
    );
    CREATE INDEX IF NOT EXISTS samples_key ON samples (server, database, kind, name, at);
";

/// Entre dos muestras del mismo tipo pasa al menos esto. Abrir y cerrar la pestaña diez veces en
/// un rato no tiene que llenar el archivo ni deformar la recta con puntos pegados.
pub const MIN_GAP_SECONDS: i64 = 3_600;

/// Pasado esto una muestra ya no sirve para decir cómo crece algo hoy.
const RETENTION_SECONDS: i64 = 400 * 86_400;

pub struct Sample {
    /// `database`, `table` o `sequence`.
    pub kind: &'static str,
    pub name: String,
    pub value: f64,
}

pub struct TrendStore {
    connection: Connection,
}

impl TrendStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        crate::private::restrict(path.as_ref())?;
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < SCHEMA_VERSION {
            connection.execute_batch(SCHEMA)?;
            connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { connection })
    }

    /// Anota las muestras si pasó el intervalo mínimo desde la última **del mismo tipo** (el de la
    /// primera muestra: el mapa de espacio y las secuencias se anotan por separado). Devuelve si
    /// anotó algo.
    pub fn record(
        &mut self,
        server: &str,
        database: &str,
        now: i64,
        samples: &[Sample],
    ) -> Result<bool> {
        let Some(first) = samples.first() else {
            return Ok(false);
        };
        let last: Option<i64> = self.connection.query_row(
            "SELECT max(at) FROM samples WHERE server = ?1 AND database = ?2 AND kind = ?3",
            params![server, database, first.kind],
            |row| row.get(0),
        )?;
        if last.is_some_and(|at| now - at < MIN_GAP_SECONDS) {
            return Ok(false);
        }
        let tx = self.connection.transaction()?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO samples (server, database, kind, name, at, value)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for sample in samples {
                insert.execute(params![
                    server,
                    database,
                    sample.kind,
                    sample.name,
                    now,
                    sample.value
                ])?;
            }
        }
        tx.execute(
            "DELETE FROM samples WHERE at < ?1",
            params![now - RETENTION_SECONDS],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// La serie de una cosa, de la más vieja a la más nueva.
    pub fn series(
        &self,
        server: &str,
        database: &str,
        kind: &str,
        name: &str,
    ) -> Result<Vec<(i64, f64)>> {
        let mut statement = self.connection.prepare(
            "SELECT at, value FROM samples
              WHERE server = ?1 AND database = ?2 AND kind = ?3 AND name = ?4
              ORDER BY at",
        )?;
        let rows = statement.query_map(params![server, database, kind, name], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// Hacia dónde va una serie.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    /// Pendiente de la recta de mejor ajuste, en unidades por día.
    pub per_day: f64,
    pub samples: usize,
    /// Cuántos días abarcan las muestras: una recta de seis horas no es una tendencia.
    pub span_days: f64,
    /// Días hasta llegar al límite al ritmo medido; `None` sin límite o si no se acerca.
    pub days_to_limit: Option<f64>,
}

/// Mínimos cuadrados sobre las muestras. No hay forma de saber el ritmo con menos de tres puntos
/// repartidos en medio día, así que antes de eso devuelve `None` y la interfaz dice «faltan datos».
pub fn forecast(series: &[(i64, f64)], limit: Option<f64>) -> Option<Forecast> {
    let (&(first, _), &(last_at, last_value)) = (series.first()?, series.last()?);
    let span_days = (last_at - first) as f64 / 86_400.0;
    if series.len() < 3 || span_days < 0.25 {
        return None;
    }
    let n = series.len() as f64;
    let xs: Vec<f64> = series
        .iter()
        .map(|(at, _)| (*at - first) as f64 / 86_400.0)
        .collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = series.iter().map(|(_, v)| v).sum::<f64>() / n;
    let numerator: f64 = xs
        .iter()
        .zip(series)
        .map(|(x, (_, y))| (x - mean_x) * (y - mean_y))
        .sum();
    let denominator: f64 = xs.iter().map(|x| (x - mean_x).powi(2)).sum();
    if denominator == 0.0 {
        return None;
    }
    let per_day = numerator / denominator;
    let days_to_limit = limit
        .filter(|limit| per_day > 0.0 && last_value < *limit)
        .map(|limit| (limit - last_value) / per_day);
    Some(Forecast {
        per_day,
        samples: series.len(),
        span_days,
        days_to_limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn la_recta_recupera_el_ritmo_de_una_serie_lineal() {
        let series: Vec<_> = (0..5).map(|d| (d * DAY, 100.0 + 10.0 * d as f64)).collect();
        let f = forecast(&series, Some(200.0)).unwrap();
        assert!((f.per_day - 10.0).abs() < 1e-9);
        // Último valor 140: faltan 60 a 10 por día.
        assert!((f.days_to_limit.unwrap() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn con_pocas_muestras_o_muy_juntas_no_hay_tendencia() {
        assert!(forecast(&[(0, 1.0), (DAY, 2.0)], None).is_none());
        let juntas = [(0, 1.0), (60, 2.0), (120, 3.0)];
        assert!(forecast(&juntas, None).is_none());
    }

    #[test]
    fn una_serie_que_baja_no_se_agota() {
        let series: Vec<_> = (0..4).map(|d| (d * DAY, 100.0 - d as f64)).collect();
        let f = forecast(&series, Some(200.0)).unwrap();
        assert!(f.per_day < 0.0);
        assert!(f.days_to_limit.is_none());
    }

    fn sample(kind: &'static str, value: f64) -> Sample {
        Sample {
            kind,
            name: "x".into(),
            value,
        }
    }

    #[test]
    fn no_anota_dos_veces_dentro_de_la_hora_y_cuenta_cada_tipo_por_separado() {
        let mut store = TrendStore::open_in_memory().unwrap();
        assert!(store
            .record("s", "db", 1_000, &[sample("table", 1.0)])
            .unwrap());
        assert!(!store
            .record("s", "db", 1_500, &[sample("table", 2.0)])
            .unwrap());
        // Otro tipo en el mismo instante: es otra lectura, no una repetición.
        assert!(store
            .record("s", "db", 1_500, &[sample("sequence", 2.0)])
            .unwrap());
        assert!(store
            .record("s", "db", 1_000 + MIN_GAP_SECONDS, &[sample("table", 3.0)])
            .unwrap());
        let series = store.series("s", "db", "table", "x").unwrap();
        assert_eq!(series, vec![(1_000, 1.0), (4_600, 3.0)]);
    }

    #[test]
    fn descarta_las_muestras_demasiado_viejas() {
        let mut store = TrendStore::open_in_memory().unwrap();
        store.record("s", "db", 0, &[sample("table", 1.0)]).unwrap();
        store
            .record("s", "db", RETENTION_SECONDS + 10, &[sample("table", 2.0)])
            .unwrap();
        assert_eq!(store.series("s", "db", "table", "x").unwrap().len(), 1);
    }
}
