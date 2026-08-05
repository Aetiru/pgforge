//! Estadísticas de uso de tablas e índices.

use serde::Serialize;
use tokio_postgres::Client;

use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStat {
    pub schema: String,
    pub table: String,
    pub live_tuples: i64,
    pub dead_tuples: i64,
    /// Proporción de tuplas muertas sobre el total.
    ///
    /// Es una **estimación** basada en los contadores de `pg_stat_user_tables`, no una medición
    /// del espacio desperdiciado: los contadores se reinician con las estadísticas y no ven el
    /// espacio libre real de cada página. Para un número exacto hay que usar `pgstattuple`, que
    /// recorre la tabla entera. Sirve para saber a qué tabla mirarle el vacuum, no para decidir un
    /// `VACUUM FULL` a ciegas.
    pub dead_ratio: Option<f64>,
    pub total_bytes: i64,
    pub table_bytes: i64,
    pub index_bytes: i64,
    pub sequential_scans: i64,
    pub index_scans: Option<i64>,
    pub last_vacuum_seconds: Option<f64>,
    pub last_autovacuum_seconds: Option<f64>,
    pub last_analyze_seconds: Option<f64>,
}

pub async fn tables(client: &Client, limit: i64) -> Result<Vec<TableStat>> {
    let rows = client
        .query(
            "SELECT s.schemaname::text,
                    s.relname::text,
                    s.n_live_tup,
                    s.n_dead_tup,
                    CASE WHEN s.n_live_tup + s.n_dead_tup > 0
                         THEN s.n_dead_tup::float8 / (s.n_live_tup + s.n_dead_tup) END,
                    pg_catalog.pg_total_relation_size(s.relid),
                    pg_catalog.pg_table_size(s.relid),
                    pg_catalog.pg_indexes_size(s.relid),
                    s.seq_scan,
                    s.idx_scan,
                    extract(epoch from (now() - s.last_vacuum))::float8,
                    extract(epoch from (now() - s.last_autovacuum))::float8,
                    extract(epoch from (now() - greatest(s.last_analyze, s.last_autoanalyze)))::float8
               FROM pg_catalog.pg_stat_user_tables s
              ORDER BY s.n_dead_tup DESC, pg_catalog.pg_total_relation_size(s.relid) DESC
              LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| TableStat {
            schema: row.get(0),
            table: row.get(1),
            live_tuples: row.get(2),
            dead_tuples: row.get(3),
            dead_ratio: row.get(4),
            total_bytes: row.get(5),
            table_bytes: row.get(6),
            index_bytes: row.get(7),
            sequential_scans: row.get(8),
            index_scans: row.get(9),
            last_vacuum_seconds: row.get(10),
            last_autovacuum_seconds: row.get(11),
            last_analyze_seconds: row.get(12),
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStat {
    pub schema: String,
    pub table: String,
    pub index: String,
    pub scans: i64,
    pub bytes: i64,
    pub is_unique: bool,
    pub is_primary: bool,
    pub is_valid: bool,
}

impl IndexStat {
    /// Un índice que nunca se usó y que no sostiene una restricción es espacio y trabajo de
    /// escritura a cambio de nada. Los índices únicos y de clave primaria quedan afuera porque su
    /// razón de ser no es acelerar consultas.
    pub fn is_unused(&self) -> bool {
        self.scans == 0 && !self.is_unique && !self.is_primary
    }
}

pub async fn indexes(client: &Client, limit: i64) -> Result<Vec<IndexStat>> {
    let rows = client
        .query(
            "SELECT s.schemaname::text,
                    s.relname::text,
                    s.indexrelname::text,
                    s.idx_scan,
                    pg_catalog.pg_relation_size(s.indexrelid),
                    i.indisunique,
                    i.indisprimary,
                    i.indisvalid
               FROM pg_catalog.pg_stat_user_indexes s
               JOIN pg_catalog.pg_index i ON i.indexrelid = s.indexrelid
              ORDER BY s.idx_scan ASC, pg_catalog.pg_relation_size(s.indexrelid) DESC
              LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| IndexStat {
            schema: row.get(0),
            table: row.get(1),
            index: row.get(2),
            scans: row.get::<_, Option<i64>>(3).unwrap_or(0),
            bytes: row.get(4),
            is_unique: row.get(5),
            is_primary: row.get(6),
            is_valid: row.get(7),
        })
        .collect())
}

/// Bloat estimado de una tabla: espacio que ocupa de más por tuplas muertas y huecos que el vacuum
/// no devolvió al disco.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableBloat {
    pub schema: String,
    pub table: String,
    pub total_bytes: i64,
    /// Espacio libre estimado dentro de la tabla, en bytes: reutilizable por filas nuevas, pero no
    /// devuelto al sistema operativo hasta un `VACUUM FULL`.
    pub free_bytes: i64,
    /// Fracción del espacio que está libre (0 a 1), no el porcentaje: se normaliza acá para que la
    /// interfaz la trate igual que el `dead_ratio` de `TableStat`.
    pub free_ratio: f64,
    /// Fracción ocupada por tuplas muertas que el vacuum todavía no limpió (0 a 1).
    pub dead_ratio: f64,
}

/// `true` si la extensión `pgstattuple` está instalada en esta base.
pub async fn has_pgstattuple(client: &Client) -> Result<bool> {
    let row = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_extension WHERE extname = 'pgstattuple')",
            &[],
        )
        .await?;
    Ok(row.get(0))
}

/// Bloat de las tablas más grandes, medido con `pgstattuple_approx`.
///
/// A diferencia de la proporción de tuplas muertas de [`tables`] —que se deriva de contadores que se
/// reinician con las estadísticas—, esto es una medición real del espacio: `pgstattuple_approx` mira
/// el mapa de visibilidad y el de espacio libre en vez de recorrer la tabla entera, así que da un
/// número honesto sin el costo del `pgstattuple` exacto. Se limita a las tablas más grandes porque
/// son las únicas donde el bloat pesa y para no llamar a la función sobre miles de relaciones.
pub async fn bloat(client: &Client, limit: i64) -> Result<Vec<TableBloat>> {
    let rows = client
        .query(
            "WITH grandes AS (
                 SELECT c.oid, n.nspname, c.relname
                   FROM pg_catalog.pg_class c
                   JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                  WHERE c.relkind IN ('r', 'm')
                    AND n.nspname NOT IN ('pg_catalog', 'information_schema')
                    AND n.nspname NOT LIKE 'pg_temp%'
                  ORDER BY pg_catalog.pg_total_relation_size(c.oid) DESC
                  LIMIT $1
             )
             SELECT g.nspname::text,
                    g.relname::text,
                    pg_catalog.pg_total_relation_size(g.oid),
                    a.approx_free_space::int8,
                    a.approx_free_percent / 100.0,
                    a.dead_tuple_percent / 100.0
               FROM grandes g
               CROSS JOIN LATERAL pgstattuple_approx(g.oid) a
              ORDER BY a.approx_free_space DESC",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| TableBloat {
            schema: row.get(0),
            table: row.get(1),
            total_bytes: row.get(2),
            free_bytes: row.get(3),
            free_ratio: row.get(4),
            dead_ratio: row.get(5),
        })
        .collect())
}

/// Consultas más costosas, si la extensión `pg_stat_statements` está instalada.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementStat {
    /// `pg_stat_statements` identifica cada fila por (usuario, base, queryid): el mismo texto
    /// normalizado aparece repetido para cada combinación, y sin estos campos las filas son
    /// indistinguibles en pantalla.
    pub query_id: Option<i64>,
    pub database: Option<String>,
    pub user: Option<String>,
    pub query: String,
    pub calls: i64,
    pub total_ms: f64,
    pub mean_ms: f64,
    pub rows: i64,
}

/// `true` si `pg_stat_statements` está instalada en esta base.
pub async fn has_statement_stats(client: &Client) -> Result<bool> {
    let row = client
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM pg_catalog.pg_extension WHERE extname = 'pg_stat_statements')",
            &[],
        )
        .await?;
    Ok(row.get(0))
}

pub async fn statements(client: &Client, limit: i64) -> Result<Vec<StatementStat>> {
    // Los nombres de columna cambiaron en PostgreSQL 13 (`total_time` pasó a `total_exec_time`),
    // pero la versión mínima soportada ya usa los nuevos.
    let rows = client
        .query(
            "SELECT s.queryid,
                    d.datname::text,
                    r.rolname::text,
                    s.query,
                    s.calls,
                    s.total_exec_time,
                    s.mean_exec_time,
                    s.rows
               FROM pg_stat_statements s
               LEFT JOIN pg_catalog.pg_database d ON d.oid = s.dbid
               LEFT JOIN pg_catalog.pg_roles r ON r.oid = s.userid
              ORDER BY s.total_exec_time DESC
              LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| StatementStat {
            query_id: row.get(0),
            database: row.get(1),
            user: row.get(2),
            query: row.get(3),
            calls: row.get(4),
            total_ms: row.get(5),
            mean_ms: row.get(6),
            rows: row.get(7),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(scans: i64, unique: bool, primary: bool) -> IndexStat {
        IndexStat {
            schema: "public".into(),
            table: "t".into(),
            index: "i".into(),
            scans,
            bytes: 8192,
            is_unique: unique,
            is_primary: primary,
            is_valid: true,
        }
    }

    #[test]
    fn solo_marca_sin_uso_los_indices_que_se_pueden_borrar() {
        assert!(index(0, false, false).is_unused());
        assert!(!index(1, false, false).is_unused());
        // Estos sostienen una restricción: que nadie los consulte no los hace prescindibles.
        assert!(!index(0, true, false).is_unused());
        assert!(!index(0, false, true).is_unused());
    }
}
