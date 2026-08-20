//! Estadísticas de uso de tablas e índices.

use serde::Serialize;
use tokio_postgres::Client;

use crate::error::{Error, Result};

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

/// La forma de un índice, que es lo que hace falta para saber si sobra.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexShape {
    pub schema: String,
    pub table: String,
    pub name: String,
    /// Las columnas o expresiones de la clave, en orden. Sin las de un `INCLUDE`: esas acompañan al
    /// índice pero no se puede buscar por ellas, así que no cuentan para decidir si uno cubre a otro.
    pub columns: Vec<String>,
    pub method: String,
    pub unique: bool,
    pub primary: bool,
    /// `true` si además arrastra columnas de un `INCLUDE`.
    pub covering: bool,
    /// El predicado, si es parcial. Dos índices con predicados distintos no se cubren entre sí.
    pub predicate: Option<String>,
    pub bytes: i64,
    pub scans: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RedundancyKind {
    /// Las mismas columnas, en el mismo orden: uno de los dos es una copia del otro.
    Duplicate,
    /// Sus columnas son el principio de las del otro, así que el otro sirve para lo mismo.
    Prefix,
}

/// Un índice que otro ya cubre.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Redundancy {
    pub schema: String,
    pub table: String,
    /// El que sobra.
    pub index: String,
    /// El que ya hace su trabajo.
    pub covered_by: String,
    pub kind: RedundancyKind,
    pub bytes: i64,
    pub scans: i64,
    /// La sentencia exacta que lo borraría, para que se vea antes de decidir.
    pub drop_sql: String,
}

/// Todos los índices válidos de las tablas del usuario, con su forma.
pub async fn index_shapes(client: &Client) -> Result<Vec<IndexShape>> {
    let rows = client
        .query(
            "SELECT n.nspname::text,
                    t.relname::text,
                    c.relname::text,
                    am.amname::text,
                    i.indisunique,
                    i.indisprimary,
                    i.indnatts > i.indnkeyatts,
                    pg_catalog.pg_get_expr(i.indpred, i.indrelid),
                    pg_catalog.pg_relation_size(i.indexrelid),
                    COALESCE(s.idx_scan, 0),
                    (SELECT array_agg(pg_catalog.pg_get_indexdef(i.indexrelid, k.ord::int, true)
                                      ORDER BY k.ord)
                       FROM generate_series(1, i.indnkeyatts) AS k(ord))
               FROM pg_catalog.pg_index i
               JOIN pg_catalog.pg_class c ON c.oid = i.indexrelid
               JOIN pg_catalog.pg_class t ON t.oid = i.indrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
               JOIN pg_catalog.pg_am am ON am.oid = c.relam
               LEFT JOIN pg_catalog.pg_stat_user_indexes s ON s.indexrelid = i.indexrelid
              WHERE i.indisvalid
                AND t.relkind IN ('r', 'm', 'p')
                AND n.nspname NOT IN ('pg_catalog', 'information_schema')
              ORDER BY n.nspname, t.relname, c.relname",
            &[],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| IndexShape {
            schema: row.get(0),
            table: row.get(1),
            name: row.get(2),
            method: row.get(3),
            unique: row.get(4),
            primary: row.get(5),
            covering: row.get(6),
            predicate: row.get(7),
            bytes: row.get(8),
            scans: row.get::<_, Option<i64>>(9).unwrap_or(0),
            // `pg_get_indexdef` por columna devuelve la expresión ya escrita: sirve igual para una
            // columna suelta que para un índice por expresión.
            columns: row.get::<_, Option<Vec<String>>>(10).unwrap_or_default(),
        })
        .collect())
}

/// Los índices que otro ya cubre, del más grande al más chico.
///
/// Es una función pura sobre lo que se leyó del catálogo: la regla es la parte que se puede
/// equivocar —y equivocarse acá significa proponer borrar un índice que hace falta—, así que se
/// prueba sin servidor.
///
/// Lo que **nunca** se propone borrar: un índice único o de clave primaria porque otro más largo lo
/// empiece (sostiene una restricción, no una consulta), uno con `INCLUDE` a favor de otro que no lo
/// tiene (se perdería el recorrido solo por índice) y cualquier par con distinto método o distinto
/// predicado, que no sirven para lo mismo.
pub fn redundancies(indexes: &[IndexShape]) -> Vec<Redundancy> {
    let mut out: Vec<Redundancy> = Vec::new();

    for (i, candidate) in indexes.iter().enumerate() {
        for (j, other) in indexes.iter().enumerate() {
            if i == j || !comparable(candidate, other) {
                continue;
            }

            let duplicate = candidate.columns == other.columns;
            let prefix = !duplicate
                && candidate.method == "btree"
                && other.columns.starts_with(&candidate.columns);
            if !duplicate && !prefix {
                continue;
            }

            // Con las mismas columnas hay que elegir cuál se queda, y decidirlo dos veces —una por
            // cada orden del par— marcaría los dos. Gana el que sostiene una restricción, después el
            // que cubre, después el más usado, y al final el nombre, que siempre desempata.
            if duplicate && keeps(candidate, other) {
                continue;
            }
            if (candidate.unique || candidate.primary) && !duplicate {
                continue;
            }
            if candidate.covering && !other.covering {
                continue;
            }

            out.push(Redundancy {
                schema: candidate.schema.clone(),
                table: candidate.table.clone(),
                index: candidate.name.clone(),
                covered_by: other.name.clone(),
                kind: if duplicate {
                    RedundancyKind::Duplicate
                } else {
                    RedundancyKind::Prefix
                },
                bytes: candidate.bytes,
                scans: candidate.scans,
                drop_sql: crate::ddl::index::drop_sql(
                    &candidate.schema,
                    &candidate.name,
                    false,
                    true,
                )
                .map(|statement| statement.sql)
                .unwrap_or_default(),
            });
            break;
        }
    }

    // Del que más ocupa al que menos: es el orden en que uno decide qué borrar primero.
    out.sort_by_key(|item| std::cmp::Reverse(item.bytes));
    out
}

/// Dos índices se pueden comparar solo si son de la misma tabla, del mismo tipo y con el mismo
/// predicado: uno parcial y uno completo responden preguntas distintas.
fn comparable(a: &IndexShape, b: &IndexShape) -> bool {
    a.schema == b.schema
        && a.table == b.table
        && a.method == b.method
        && a.predicate == b.predicate
        && !a.columns.is_empty()
        && !b.columns.is_empty()
}

/// Con columnas idénticas, cuál de los dos se conserva.
fn keeps(candidate: &IndexShape, other: &IndexShape) -> bool {
    let rank = |index: &IndexShape| {
        (
            index.primary,
            index.unique,
            index.covering,
            index.scans,
            std::cmp::Reverse(index.name.clone()),
        )
    };
    rank(candidate) > rank(other)
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
    /// `None` cuando el archivo de textos de la extensión ya no tiene el de esta fila. La entrada
    /// con sus tiempos sigue valiendo, así que se muestra igual y sin el texto.
    pub query: Option<String>,
    pub calls: i64,
    pub total_ms: f64,
    pub mean_ms: f64,
    pub rows: i64,
}

/// Qué versión de `pg_stat_statements` está instalada en esta base, si está.
///
/// Interesa la versión de la **extensión**, no la del servidor: la vista la define el archivo de la
/// extensión, y una base creada hace años —o migrada con `pg_upgrade`— puede seguir con la 1.7
/// aunque el servidor sea la 14. Ahí las columnas todavía se llaman `total_time` y `mean_time`.
pub async fn statement_stats_version(client: &Client) -> Result<Option<String>> {
    let row = client
        .query_opt(
            "SELECT extversion FROM pg_catalog.pg_extension WHERE extname = 'pg_stat_statements'",
            &[],
        )
        .await?;
    Ok(row.map(|row| row.get(0)))
}

/// `true` si `pg_stat_statements` está instalada en esta base.
pub async fn has_statement_stats(client: &Client) -> Result<bool> {
    Ok(statement_stats_version(client).await?.is_some())
}

/// `true` si esa versión de la extensión ya usa `total_exec_time` en vez de `total_time`.
///
/// El corte es la 1.8, la que vino con PostgreSQL 13: ahí se separó el tiempo de planificación del
/// de ejecución y las dos columnas se renombraron. Una versión que no se puede interpretar se
/// asume nueva, que es lo que trae cualquier servidor del rango soportado sin que nadie la fije.
fn uses_exec_time(extversion: &str) -> bool {
    let mut parts = extversion.split('.');
    let major: u32 = match parts.next().and_then(|part| part.parse().ok()) {
        Some(major) => major,
        None => return true,
    };
    // `unwrap_or(0)`: una versión sin minor (`"2"`) es posterior a cualquier `1.x`.
    let minor: u32 = parts.next().and_then(|part| part.parse().ok()).unwrap_or(0);

    (major, minor) >= (1, 8)
}

pub async fn statements(client: &Client, limit: i64) -> Result<Vec<StatementStat>> {
    let extversion = statement_stats_version(client).await?.ok_or_else(|| {
        Error::Config("la extensión pg_stat_statements no está instalada en esta base".to_owned())
    })?;

    // Los nombres salen de una lista cerrada de dos, no de nada que escriba el usuario.
    let (total, mean) = if uses_exec_time(&extversion) {
        ("total_exec_time", "mean_exec_time")
    } else {
        ("total_time", "mean_time")
    };

    let rows = client
        .query(
            &format!(
                "SELECT s.queryid,
                        d.datname::text,
                        r.rolname::text,
                        s.query,
                        s.calls,
                        s.{total},
                        s.{mean},
                        s.rows
                   FROM pg_stat_statements s
                   LEFT JOIN pg_catalog.pg_database d ON d.oid = s.dbid
                   LEFT JOIN pg_catalog.pg_roles r ON r.oid = s.userid
                  ORDER BY s.{total} DESC
                  LIMIT $1"
            ),
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

    fn shape(name: &str, columns: &[&str]) -> IndexShape {
        IndexShape {
            schema: "public".into(),
            table: "trabajos".into(),
            name: name.into(),
            columns: columns.iter().map(|c| (*c).to_string()).collect(),
            method: "btree".into(),
            unique: false,
            primary: false,
            covering: false,
            predicate: None,
            bytes: 1024,
            scans: 0,
        }
    }

    #[test]
    fn dos_indices_con_las_mismas_columnas_dejan_uno_solo() {
        let indexes = vec![shape("idx_a", &["estado"]), shape("idx_b", &["estado"])];
        let sobran = redundancies(&indexes);

        assert_eq!(sobran.len(), 1, "solo uno de los dos sobra: {sobran:?}");
        assert_eq!(sobran[0].kind, RedundancyKind::Duplicate);
        assert!(sobran[0].drop_sql.contains("DROP INDEX"));
    }

    #[test]
    fn el_que_sostiene_una_restriccion_es_el_que_se_queda() {
        let mut unico = shape("trabajos_estado_key", &["estado"]);
        unico.unique = true;
        let indexes = vec![shape("idx_estado", &["estado"]), unico];

        let sobran = redundancies(&indexes);
        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_estado");
        assert_eq!(sobran[0].covered_by, "trabajos_estado_key");
    }

    #[test]
    fn un_indice_es_prescindible_si_otro_lo_empieza() {
        let indexes = vec![
            shape("idx_estado", &["estado"]),
            shape("idx_estado_fecha", &["estado", "fecha"]),
        ];

        let sobran = redundancies(&indexes);
        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_estado");
        assert_eq!(sobran[0].kind, RedundancyKind::Prefix);
    }

    #[test]
    fn el_orden_de_las_columnas_importa() {
        // `(fecha, estado)` no sirve para buscar solo por `estado`: no es un prefijo.
        let indexes = vec![
            shape("idx_estado", &["estado"]),
            shape("idx_fecha_estado", &["fecha", "estado"]),
        ];

        assert!(redundancies(&indexes).is_empty());
    }

    #[test]
    fn un_indice_unico_no_se_borra_porque_otro_lo_empiece() {
        let mut unico = shape("trabajos_codigo_key", &["codigo"]);
        unico.unique = true;
        let indexes = vec![unico, shape("idx_codigo_fecha", &["codigo", "fecha"])];

        assert!(
            redundancies(&indexes).is_empty(),
            "borrarlo sacaría la restricción de unicidad"
        );
    }

    #[test]
    fn no_se_comparan_indices_de_distinto_tipo_ni_con_distinto_predicado() {
        let mut gin = shape("idx_gin", &["datos"]);
        gin.method = "gin".into();
        assert!(redundancies(&[shape("idx_btree", &["datos"]), gin]).is_empty());

        let mut parcial = shape("idx_parcial", &["estado"]);
        parcial.predicate = Some("(activo)".into());
        assert!(redundancies(&[shape("idx_todo", &["estado"]), parcial]).is_empty());
    }

    #[test]
    fn el_que_cubre_con_include_no_se_cambia_por_uno_que_no_cubre() {
        let mut cubre = shape("idx_cubre", &["estado"]);
        cubre.covering = true;
        let sobran = redundancies(&[cubre, shape("idx_pelado", &["estado"])]);

        assert_eq!(sobran.len(), 1);
        assert_eq!(sobran[0].index, "idx_pelado");
    }

    #[test]
    fn los_indices_de_tablas_distintas_no_se_estorban() {
        let mut otra = shape("idx_estado", &["estado"]);
        otra.table = "clientes".into();

        assert!(redundancies(&[shape("idx_estado", &["estado"]), otra]).is_empty());
    }

    #[test]
    fn las_columnas_de_pg_stat_statements_salen_de_la_version_de_la_extension() {
        // El corte está en la 1.8, la de PostgreSQL 13.
        assert!(!uses_exec_time("1.7"), "la 1.7 todavía tiene total_time");
        assert!(!uses_exec_time("1.6"));
        assert!(uses_exec_time("1.8"));
        assert!(uses_exec_time("1.9"));
        // Como texto, "1.10" sería anterior a "1.9": se comparan como números.
        assert!(uses_exec_time("1.10"));
        assert!(uses_exec_time("1.11"));
        assert!(uses_exec_time("2.0"));
        // Sin minor y sin poder interpretarla se asume nueva, que es lo que trae cualquier
        // servidor del rango soportado.
        assert!(uses_exec_time("2"));
        assert!(uses_exec_time("vaya a saber"));
    }
}
