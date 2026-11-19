//! Diagnósticos del monitoreo: lo que no se ve mirando una sesión.
//!
//! Secuencias por agotarse, edad de las transacciones, cuándo le toca el autovacuum a cada tabla,
//! caché, quién usa las conexiones, mapa de espacio y checkpoints. Mismo reparto que el resto del
//! monitoreo: leer el catálogo es lo único que habla con el servidor, y **decidir** —qué cuenta como
//! para revisar— vive en funciones puras que se prueban sin conexión. Los umbrales son altos a
//! propósito: una lista de veinte avisos tibios se ignora entera.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use tokio_postgres::Client;

use super::activity::Backend;
use crate::caps::ServerCaps;
use crate::error::Result;

/// Qué tan grave es lo que se muestra. `Info` no pide hacer nada: es algo bueno de saber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Ok,
    Info,
    Review,
    Attention,
}

const SYSTEM_SCHEMAS: &str = "('pg_catalog', 'information_schema')";

// ---------------------------------------------------------------------------------------------
// Secuencias
// ---------------------------------------------------------------------------------------------

/// Una secuencia y cuánto de su rango ya gastó.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceUsage {
    pub schema: String,
    pub name: String,
    /// `smallint`, `integer` o `bigint`: lo que decide el techo.
    pub data_type: String,
    /// `None` si nunca se pidió un valor, o si el rol no puede leerla.
    pub last_value: Option<i64>,
    pub max_value: i64,
    pub increment_by: i64,
    pub cycle: bool,
    /// Fracción del rango ya usada (0 a 1).
    pub used_ratio: f64,
    /// Cuántos valores más puede dar antes de llegar al final.
    pub remaining: i64,
    pub level: Level,
    /// Crecimiento por día, medido con las muestras guardadas; `None` mientras faltan datos.
    pub per_day: Option<f64>,
    /// Días hasta agotarse al ritmo medido.
    pub days_left: Option<f64>,
}

/// Cuánto del rango gastó una secuencia y cuántos valores le quedan.
///
/// `i128` porque el rango de un `bigint` entra justo en `i64` y `max - min` ya no.
pub fn sequence_position(last: Option<i64>, min: i64, max: i64, increment: i64) -> (f64, i64) {
    let span = i128::from(max) - i128::from(min);
    if span <= 0 || increment == 0 {
        return (0.0, 0);
    }
    let Some(last) = last else {
        return (0.0, saturate(span / i128::from(increment.unsigned_abs())));
    };
    let (used, left) = if increment > 0 {
        (
            i128::from(last) - i128::from(min),
            (i128::from(max) - i128::from(last)) / i128::from(increment),
        )
    } else {
        (
            i128::from(max) - i128::from(last),
            (i128::from(last) - i128::from(min)) / i128::from(increment.unsigned_abs()),
        )
    };
    (
        (used as f64 / span as f64).clamp(0.0, 1.0),
        saturate(left.max(0)),
    )
}

fn saturate(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Una secuencia que da la vuelta no se agota: a lo sumo repite números, y eso es una decisión de
/// quien la creó. Se avisa, pero no como falla.
pub fn sequence_level(used_ratio: f64, cycle: bool) -> Level {
    let level = if used_ratio >= 0.95 {
        Level::Attention
    } else if used_ratio >= 0.8 {
        Level::Review
    } else {
        Level::Ok
    };
    if cycle {
        level.min(Level::Info)
    } else {
        level
    }
}

pub async fn sequences(client: &Client, limit: i64) -> Result<Vec<SequenceUsage>> {
    let sql = format!(
        "SELECT schemaname::text, sequencename::text, data_type::text, min_value, max_value,
                increment_by, cycle, last_value
           FROM pg_catalog.pg_sequences
          WHERE schemaname NOT IN {SYSTEM_SCHEMAS}"
    );
    let rows = client.query(&sql, &[]).await?;
    let mut all: Vec<SequenceUsage> = rows
        .iter()
        .map(|row| {
            let min: i64 = row.get(3);
            let max: i64 = row.get(4);
            let increment: i64 = row.get(5);
            let cycle: bool = row.get(6);
            let last: Option<i64> = row.get(7);
            let (used_ratio, remaining) = sequence_position(last, min, max, increment);
            SequenceUsage {
                schema: row.get(0),
                name: row.get(1),
                data_type: row.get(2),
                last_value: last,
                max_value: max,
                increment_by: increment,
                cycle,
                used_ratio,
                remaining,
                level: sequence_level(used_ratio, cycle),
                per_day: None,
                days_left: None,
            }
        })
        .collect();
    // Las más gastadas primero: la lista es para encontrar las que están por terminarse.
    all.sort_by(|a, b| b.used_ratio.total_cmp(&a.used_ratio));
    all.truncate(usize::try_from(limit).unwrap_or(100));
    Ok(all)
}

// ---------------------------------------------------------------------------------------------
// Wraparound
// ---------------------------------------------------------------------------------------------

/// El identificador de transacción tiene 32 bits: a los ~2.147 millones el servidor deja de aceptar
/// escrituras hasta que alguien limpie.
const XID_LIMIT: f64 = 2_147_483_647.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WraparoundDatabase {
    pub name: String,
    pub age: i64,
    pub ratio: f64,
    pub level: Level,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WraparoundTable {
    pub schema: String,
    pub table: String,
    pub age: i64,
    pub ratio: f64,
    pub level: Level,
    pub total_bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wraparound {
    /// Edad a la que el autovacuum fuerza una limpieza aunque esté apagado (`autovacuum_freeze_max_age`).
    pub freeze_max_age: i64,
    pub databases: Vec<WraparoundDatabase>,
    pub tables: Vec<WraparoundTable>,
}

/// Umbrales altos: con el valor de fábrica el autovacuum actúa a los 200 millones, así que ver
/// 1.000 millones significa que no está pudiendo, y 1.500 que el servidor está por entrar en modo
/// de emergencia (el `failsafe` se dispara a los 1.600).
pub fn age_level(age: i64) -> Level {
    if age >= 1_500_000_000 {
        Level::Attention
    } else if age >= 1_000_000_000 {
        Level::Review
    } else {
        Level::Ok
    }
}

pub async fn wraparound(client: &Client, limit: i64) -> Result<Wraparound> {
    let freeze: Option<String> = client
        .query_opt(
            "SELECT setting FROM pg_catalog.pg_settings WHERE name = 'autovacuum_freeze_max_age'",
            &[],
        )
        .await?
        .and_then(|row| row.get(0));
    let databases = client
        .query(
            "SELECT datname::text, pg_catalog.age(datfrozenxid)::int8
               FROM pg_catalog.pg_database
              WHERE datallowconn
              ORDER BY 2 DESC",
            &[],
        )
        .await?
        .iter()
        .map(|row| {
            let age: i64 = row.get(1);
            WraparoundDatabase {
                name: row.get(0),
                age,
                ratio: age as f64 / XID_LIMIT,
                level: age_level(age),
            }
        })
        .collect();
    let sql = format!(
        "SELECT n.nspname::text, c.relname::text, pg_catalog.age(c.relfrozenxid)::int8,
                pg_catalog.pg_total_relation_size(c.oid)
           FROM pg_catalog.pg_class c
           JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
          WHERE c.relkind IN ('r', 'm')
            AND n.nspname NOT IN {SYSTEM_SCHEMAS}
            AND n.nspname !~ '^pg_toast'
          ORDER BY 3 DESC
          LIMIT $1"
    );
    let tables = client
        .query(&sql, &[&limit])
        .await?
        .iter()
        .map(|row| {
            let age: i64 = row.get(2);
            WraparoundTable {
                schema: row.get(0),
                table: row.get(1),
                age,
                ratio: age as f64 / XID_LIMIT,
                level: age_level(age),
                total_bytes: row.get(3),
            }
        })
        .collect();
    Ok(Wraparound {
        freeze_max_age: freeze.and_then(|v| v.parse().ok()).unwrap_or(200_000_000),
        databases,
        tables,
    })
}

// ---------------------------------------------------------------------------------------------
// Autovacuum
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutovacuumStatus {
    /// Apagado para esta tabla o para todo el servidor.
    Disabled,
    /// Ya pasó el umbral: le toca en el próximo ciclo.
    Due,
    /// Todavía no llegó al umbral.
    Waiting,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutovacuumRow {
    pub schema: String,
    pub table: String,
    pub live_tuples: i64,
    pub dead_tuples: i64,
    pub modified_since_analyze: i64,
    /// Filas muertas a partir de las cuales el autovacuum limpia esta tabla.
    pub vacuum_at: f64,
    pub analyze_at: f64,
    /// Cuántas filas muertas faltan para llegar al umbral (0 si ya pasó).
    pub vacuum_missing: i64,
    pub status: AutovacuumStatus,
    /// La tabla tiene sus propios `autovacuum_*` y no usa los del servidor.
    pub own_settings: bool,
    pub last_autovacuum_seconds: Option<f64>,
    pub last_autoanalyze_seconds: Option<f64>,
    pub total_bytes: i64,
    pub level: Level,
}

/// Los parámetros del servidor que fijan el umbral, con los valores de fábrica de respaldo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutovacuumSettings {
    pub enabled: bool,
    pub vacuum_threshold: f64,
    pub vacuum_scale: f64,
    pub analyze_threshold: f64,
    pub analyze_scale: f64,
}

impl AutovacuumSettings {
    pub fn from_values(values: &HashMap<String, String>) -> Self {
        let number = |name: &str, default: f64| {
            values
                .get(name)
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        Self {
            enabled: values.get("autovacuum").map(|v| v == "on").unwrap_or(true),
            vacuum_threshold: number("autovacuum_vacuum_threshold", 50.0),
            vacuum_scale: number("autovacuum_vacuum_scale_factor", 0.2),
            analyze_threshold: number("autovacuum_analyze_threshold", 50.0),
            analyze_scale: number("autovacuum_analyze_scale_factor", 0.1),
        }
    }
}

fn reloption(options: &[String], key: &str) -> Option<String> {
    options.iter().find_map(|option| {
        let (name, value) = option.split_once('=')?;
        (name == key).then(|| value.to_owned())
    })
}

/// Lo que decide cuándo le toca a una tabla, a partir de los parámetros del servidor y de los que
/// la tabla pisa con `reloptions`. Es la fórmula del propio PostgreSQL:
/// `umbral + factor × filas`.
pub fn autovacuum_plan(
    settings: &AutovacuumSettings,
    options: &[String],
    estimated_rows: f64,
    dead: i64,
) -> (f64, f64, AutovacuumStatus, bool, i64) {
    let number = |key: &str, default: f64| {
        reloption(options, key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let own = options.iter().any(|o| o.starts_with("autovacuum_"));
    // `reltuples` vale -1 en una tabla que nunca se limpió: sin estimación, 0 filas.
    let rows = estimated_rows.max(0.0);
    let vacuum_at = number("autovacuum_vacuum_threshold", settings.vacuum_threshold)
        + number("autovacuum_vacuum_scale_factor", settings.vacuum_scale) * rows;
    let analyze_at = number("autovacuum_analyze_threshold", settings.analyze_threshold)
        + number("autovacuum_analyze_scale_factor", settings.analyze_scale) * rows;
    let off =
        !settings.enabled || reloption(options, "autovacuum_enabled").as_deref() == Some("false");
    let status = if off {
        AutovacuumStatus::Disabled
    } else if dead as f64 >= vacuum_at {
        AutovacuumStatus::Due
    } else {
        AutovacuumStatus::Waiting
    };
    let missing = (vacuum_at - dead as f64).ceil().max(0.0) as i64;
    (vacuum_at, analyze_at, status, own, missing)
}

/// Una tabla que «le toca» y hace más de un día que no se limpia es la que importa: algo la frena
/// —una transacción vieja que retiene el horizonte, un bloqueo— y el autovacuum no la alcanza.
pub fn autovacuum_level(
    status: AutovacuumStatus,
    dead: i64,
    vacuum_at: f64,
    last_autovacuum_seconds: Option<f64>,
) -> Level {
    match status {
        AutovacuumStatus::Disabled if dead as f64 >= vacuum_at => Level::Review,
        AutovacuumStatus::Due
            if last_autovacuum_seconds.is_none_or(|seconds| seconds > 86_400.0) =>
        {
            Level::Review
        }
        AutovacuumStatus::Due => Level::Info,
        _ => Level::Ok,
    }
}

pub async fn autovacuum(client: &Client, limit: i64) -> Result<Vec<AutovacuumRow>> {
    let values = settings(
        client,
        &[
            "autovacuum",
            "autovacuum_vacuum_threshold",
            "autovacuum_vacuum_scale_factor",
            "autovacuum_analyze_threshold",
            "autovacuum_analyze_scale_factor",
        ],
    )
    .await?
    .into_iter()
    .filter_map(|(name, setting)| Some((name, setting.value?)))
    .collect();
    let settings = AutovacuumSettings::from_values(&values);

    let rows = client
        .query(
            "SELECT s.schemaname::text, s.relname::text, c.reltuples::float8,
                    s.n_live_tup, s.n_dead_tup, s.n_mod_since_analyze, c.reloptions,
                    extract(epoch from (now() - s.last_autovacuum))::float8,
                    extract(epoch from (now() - s.last_autoanalyze))::float8,
                    pg_catalog.pg_total_relation_size(s.relid)
               FROM pg_catalog.pg_stat_user_tables s
               JOIN pg_catalog.pg_class c ON c.oid = s.relid
              WHERE c.relkind <> 'p'
              ORDER BY s.n_dead_tup DESC
              LIMIT $1",
            &[&limit],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| {
            let options: Vec<String> = row.get::<_, Option<Vec<String>>>(6).unwrap_or_default();
            let dead: i64 = row.get(4);
            let last_autovacuum: Option<f64> = row.get(7);
            let (vacuum_at, analyze_at, status, own, missing) =
                autovacuum_plan(&settings, &options, row.get(2), dead);
            AutovacuumRow {
                schema: row.get(0),
                table: row.get(1),
                live_tuples: row.get(3),
                dead_tuples: dead,
                modified_since_analyze: row.get(5),
                vacuum_at,
                analyze_at,
                vacuum_missing: missing,
                status,
                own_settings: own,
                last_autovacuum_seconds: last_autovacuum,
                last_autoanalyze_seconds: row.get(8),
                total_bytes: row.get(9),
                level: autovacuum_level(status, dead, vacuum_at, last_autovacuum),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------------------------
// Caché
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCache {
    pub schema: String,
    pub table: String,
    pub heap_read: i64,
    pub heap_hit: i64,
    pub index_read: i64,
    pub index_hit: i64,
    pub heap_ratio: Option<f64>,
    pub index_ratio: Option<f64>,
    pub level: Level,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexCache {
    pub schema: String,
    pub table: String,
    pub index: String,
    pub read: i64,
    pub hit: i64,
    pub ratio: Option<f64>,
    pub level: Level,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheReport {
    pub tables: Vec<TableCache>,
    pub indexes: Vec<IndexCache>,
}

/// Proporción de lecturas que se resolvieron en la caché de PostgreSQL. `None` sin lecturas.
pub fn hit_ratio(read: i64, hit: i64) -> Option<f64> {
    let total = read + hit;
    (total > 0).then(|| hit as f64 / total as f64)
}

/// Un porcentaje bajo con pocas lecturas no dice nada: hace falta volumen para que importe.
/// Además, `read` incluye lo que el sistema operativo tenía en su propia caché, así que esto es un
/// aviso para mirar la tabla y no una medición del disco.
pub fn cache_level(read: i64, hit: i64) -> Level {
    match hit_ratio(read, hit) {
        Some(ratio) if read >= 10_000 && ratio < 0.9 => Level::Review,
        _ => Level::Ok,
    }
}

pub async fn cache(client: &Client, limit: i64) -> Result<CacheReport> {
    let tables = client
        .query(
            "SELECT schemaname::text, relname::text,
                    coalesce(heap_blks_read, 0), coalesce(heap_blks_hit, 0),
                    coalesce(idx_blks_read, 0), coalesce(idx_blks_hit, 0)
               FROM pg_catalog.pg_statio_user_tables
              ORDER BY coalesce(heap_blks_read, 0) + coalesce(idx_blks_read, 0) DESC
              LIMIT $1",
            &[&limit],
        )
        .await?
        .iter()
        .map(|row| {
            let (heap_read, heap_hit, index_read, index_hit): (i64, i64, i64, i64) =
                (row.get(2), row.get(3), row.get(4), row.get(5));
            TableCache {
                schema: row.get(0),
                table: row.get(1),
                heap_read,
                heap_hit,
                index_read,
                index_hit,
                heap_ratio: hit_ratio(heap_read, heap_hit),
                index_ratio: hit_ratio(index_read, index_hit),
                level: cache_level(heap_read, heap_hit).max(cache_level(index_read, index_hit)),
            }
        })
        .collect();
    let indexes = client
        .query(
            "SELECT schemaname::text, relname::text, indexrelname::text,
                    coalesce(idx_blks_read, 0), coalesce(idx_blks_hit, 0)
               FROM pg_catalog.pg_statio_user_indexes
              ORDER BY coalesce(idx_blks_read, 0) DESC
              LIMIT $1",
            &[&limit],
        )
        .await?
        .iter()
        .map(|row| {
            let (read, hit): (i64, i64) = (row.get(3), row.get(4));
            IndexCache {
                schema: row.get(0),
                table: row.get(1),
                index: row.get(2),
                read,
                hit,
                ratio: hit_ratio(read, hit),
                level: cache_level(read, hit),
            }
        })
        .collect();
    Ok(CacheReport { tables, indexes })
}

// ---------------------------------------------------------------------------------------------
// Conexiones
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionGroup {
    /// `None` en las tres: el rol no puede ver el detalle de esas sesiones.
    pub application: Option<String>,
    pub user: Option<String>,
    pub address: Option<String>,
    pub total: i64,
    pub active: i64,
    pub idle: i64,
    pub idle_in_transaction: i64,
    /// La sesión inactiva que lleva más tiempo así: la que delata una conexión olvidada.
    pub oldest_idle_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionReport {
    pub max_connections: i64,
    /// Conexiones que el servidor guarda para superusuarios (`superuser_reserved_connections`).
    pub reserved: i64,
    pub total: i64,
    pub groups: Vec<ConnectionGroup>,
}

/// Agrupa las sesiones de usuario por aplicación, rol y dirección. Pura: la lista ya viene leída.
pub fn group_connections(backends: &[Backend]) -> Vec<ConnectionGroup> {
    type Key = (Option<String>, Option<String>, Option<String>);
    let mut groups: BTreeMap<Key, ConnectionGroup> = BTreeMap::new();
    for backend in backends
        .iter()
        .filter(|b| !b.is_background() && !b.is_monitor)
    {
        let key = (
            backend.application_name.clone().filter(|a| !a.is_empty()),
            backend.user.clone(),
            backend.client_addr.clone(),
        );
        let group = groups
            .entry(key.clone())
            .or_insert_with(|| ConnectionGroup {
                application: key.0.clone(),
                user: key.1.clone(),
                address: key.2.clone(),
                total: 0,
                active: 0,
                idle: 0,
                idle_in_transaction: 0,
                oldest_idle_seconds: None,
            });
        group.total += 1;
        match backend.state.as_deref() {
            Some("active") => group.active += 1,
            Some("idle") => {
                group.idle += 1;
                if let Some(seconds) = backend.state_seconds {
                    group.oldest_idle_seconds = Some(
                        group
                            .oldest_idle_seconds
                            .map_or(seconds, |old| old.max(seconds)),
                    );
                }
            }
            Some(state) if state.starts_with("idle in transaction") => {
                group.idle_in_transaction += 1
            }
            _ => {}
        }
    }
    let mut list: Vec<_> = groups.into_values().collect();
    list.sort_by_key(|g| std::cmp::Reverse(g.total));
    list
}

pub async fn connections(client: &Client, caps: &ServerCaps) -> Result<ConnectionReport> {
    let backends = super::activity::backends(client, caps).await?;
    let values = settings(
        client,
        &["max_connections", "superuser_reserved_connections"],
    )
    .await?;
    let number = |name: &str| {
        values
            .get(name)
            .and_then(|s| s.value.as_deref())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    };
    let groups = group_connections(&backends);
    Ok(ConnectionReport {
        max_connections: number("max_connections"),
        reserved: number("superuser_reserved_connections"),
        total: groups.iter().map(|g| g.total).sum(),
        groups,
    })
}

// ---------------------------------------------------------------------------------------------
// Mapa de espacio
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceEntry {
    pub schema: String,
    pub table: String,
    /// `r` (tabla) o `m` (vista materializada).
    pub kind: String,
    pub heap_bytes: i64,
    /// TOAST, mapa de espacio libre y de visibilidad: todo lo de la tabla que no es la tabla ni un índice.
    pub toast_bytes: i64,
    pub index_bytes: i64,
}

impl SpaceEntry {
    pub fn total(&self) -> i64 {
        self.heap_bytes + self.toast_bytes + self.index_bytes
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceMap {
    pub database: String,
    pub database_bytes: i64,
    pub entries: Vec<SpaceEntry>,
    /// Lo que la base ocupa y no está en la lista: el catálogo, las tablas más chicas que el corte.
    pub rest_bytes: i64,
}

pub async fn space_map(client: &Client, limit: i64) -> Result<SpaceMap> {
    let head = client
        .query_one(
            "SELECT pg_catalog.current_database()::text,
                    pg_catalog.pg_database_size(pg_catalog.current_database())",
            &[],
        )
        .await?;
    let sql = format!(
        "SELECT n.nspname::text, c.relname::text, c.relkind::text,
                pg_catalog.pg_relation_size(c.oid),
                pg_catalog.pg_table_size(c.oid) - pg_catalog.pg_relation_size(c.oid),
                pg_catalog.pg_indexes_size(c.oid)
           FROM pg_catalog.pg_class c
           JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
          WHERE c.relkind IN ('r', 'm')
            AND n.nspname NOT IN {SYSTEM_SCHEMAS}
            AND n.nspname !~ '^pg_toast'
          ORDER BY pg_catalog.pg_total_relation_size(c.oid) DESC
          LIMIT $1"
    );
    let entries: Vec<SpaceEntry> = client
        .query(&sql, &[&limit])
        .await?
        .iter()
        .map(|row| SpaceEntry {
            schema: row.get(0),
            table: row.get(1),
            kind: row.get(2),
            heap_bytes: row.get(3),
            toast_bytes: row.get(4),
            index_bytes: row.get(5),
        })
        .collect();
    let database_bytes: i64 = head.get(1);
    let listed: i64 = entries.iter().map(SpaceEntry::total).sum();
    Ok(SpaceMap {
        database: head.get(0),
        database_bytes,
        entries,
        rest_bytes: (database_bytes - listed).max(0),
    })
}

// ---------------------------------------------------------------------------------------------
// Parámetros del servidor
// ---------------------------------------------------------------------------------------------

/// Una fila de `pg_settings`.
#[derive(Debug, Clone)]
pub struct Setting {
    /// `None` si el rol no puede ver este parámetro (`pg_read_all_settings`).
    pub value: Option<String>,
    pub unit: Option<String>,
    /// `default`, `configuration file`, `override`…: dice si alguien lo tocó.
    pub source: String,
    pub boot_val: Option<String>,
    /// `postmaster` pide reiniciar el servidor; `sighup` alcanza con recargar.
    pub context: String,
}

pub async fn settings(client: &Client, names: &[&str]) -> Result<HashMap<String, Setting>> {
    let names: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
    let rows = client
        .query(
            "SELECT name::text, setting::text, unit::text, source::text, boot_val::text,
                    context::text
               FROM pg_catalog.pg_settings
              WHERE name = ANY($1)",
            &[&names],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| {
            (
                row.get::<_, String>(0),
                Setting {
                    value: row.get(1),
                    unit: row.get(2),
                    source: row.get(3),
                    boot_val: row.get(4),
                    context: row.get(5),
                },
            )
        })
        .collect())
}

/// Escribe un tamaño como lo acepta `ALTER SYSTEM`: la unidad más grande en la que cabe entero.
pub fn pg_size(bytes: i64) -> String {
    const UNITS: [(i64, &str); 3] = [(1 << 30, "GB"), (1 << 20, "MB"), (1 << 10, "kB")];
    for (size, name) in UNITS {
        if bytes >= size && bytes % size == 0 {
            return format!("{}{name}", bytes / size);
        }
    }
    // No entra entero en ninguna: se baja a kB, que es la unidad más chica que acepta.
    format!("{}kB", (bytes / 1024).max(1))
}

/// Un tamaño para leer, con coma decimal.
pub fn human_bytes(bytes: i64) -> String {
    let value = bytes as f64;
    let (number, unit) = if value >= (1u64 << 40) as f64 {
        (value / (1u64 << 40) as f64, "TB")
    } else if value >= (1u64 << 30) as f64 {
        (value / (1u64 << 30) as f64, "GB")
    } else if value >= (1u64 << 20) as f64 {
        (value / (1u64 << 20) as f64, "MB")
    } else {
        (value / 1024.0, "kB")
    };
    let text = if (number - number.round()).abs() < 0.05 {
        format!("{}", number.round() as i64)
    } else {
        format!("{number:.1}").replace('.', ",")
    };
    format!("{text} {unit}")
}

/// El valor de un parámetro de memoria en bytes, según su unidad (`8kB`, `kB`, `MB`…).
pub fn setting_bytes(setting: &Setting) -> Option<i64> {
    let value: f64 = setting.value.as_deref()?.parse().ok()?;
    let unit = setting.unit.as_deref()?;
    let digits: String = unit.chars().take_while(|c| c.is_ascii_digit()).collect();
    let multiplier: f64 = if digits.is_empty() {
        1.0
    } else {
        digits.parse().ok()?
    };
    let base: f64 = match &unit[digits.len()..] {
        "B" => 1.0,
        "kB" => 1024.0,
        "MB" => 1_048_576.0,
        "GB" => 1_073_741_824.0,
        "TB" => 1_099_511_627_776.0,
        _ => return None,
    };
    Some((value * multiplier * base) as i64)
}

// ---------------------------------------------------------------------------------------------
// Checkpoints y WAL
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalReport {
    pub records: i64,
    pub full_page_images: i64,
    pub bytes: f64,
    pub buffers_full: i64,
    pub reset_seconds: Option<f64>,
    pub bytes_per_second: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointReport {
    pub timed: i64,
    pub requested: i64,
    /// Fracción de los checkpoints que se dispararon porque se llenó `max_wal_size`.
    pub forced_ratio: Option<f64>,
    pub write_seconds: f64,
    pub sync_seconds: f64,
    pub buffers_written: i64,
    pub reset_seconds: Option<f64>,
    pub max_wal_size_bytes: Option<i64>,
    pub checkpoint_timeout_seconds: Option<i64>,
    pub completion_target: Option<f64>,
    /// `None` en PG 13, que no tiene `pg_stat_wal`.
    pub wal: Option<WalReport>,
    pub level: Level,
    pub advice: Vec<String>,
}

/// Qué decir de los checkpoints. Forzados son los que no esperaron a `checkpoint_timeout`: el WAL
/// llenó `max_wal_size` antes, y cada uno escribe de golpe en vez de repartirlo en el tiempo.
pub fn checkpoint_advice(
    timed: i64,
    requested: i64,
    max_wal_size_bytes: Option<i64>,
    completion_target: Option<f64>,
) -> (Level, Vec<String>) {
    let mut level = Level::Ok;
    let mut advice = Vec::new();
    let total = timed + requested;
    // Pocos checkpoints no dicen nada: un servidor recién reiniciado tiene uno o dos.
    if total >= 10 {
        let ratio = requested as f64 / total as f64;
        if ratio >= 0.3 {
            level = if ratio >= 0.7 {
                Level::Attention
            } else {
                Level::Review
            };
            let hoy = max_wal_size_bytes
                .map(|b| format!(" (hoy {})", human_bytes(b)))
                .unwrap_or_default();
            advice.push(format!(
                "{} % de los checkpoints se forzaron porque el WAL llenó max_wal_size{hoy}. \
                 Subirlo reparte la escritura en el tiempo y baja los picos de disco.",
                (ratio * 100.0).round() as i64
            ));
        }
    }
    if completion_target.is_some_and(|target| target < 0.9) {
        level = level.max(Level::Info);
        advice.push(
            "checkpoint_completion_target está por debajo de 0,9: la escritura de cada \
             checkpoint se concentra en vez de repartirse."
                .to_owned(),
        );
    }
    (level, advice)
}

pub async fn checkpoints(client: &Client, caps: &ServerCaps) -> Result<CheckpointReport> {
    let sql = if caps.has_pg_stat_checkpointer() {
        "SELECT num_timed, num_requested, write_time, sync_time, buffers_written,
                extract(epoch from (now() - stats_reset))::float8
           FROM pg_catalog.pg_stat_checkpointer"
    } else {
        "SELECT checkpoints_timed, checkpoints_req, checkpoint_write_time, checkpoint_sync_time,
                buffers_checkpoint, extract(epoch from (now() - stats_reset))::float8
           FROM pg_catalog.pg_stat_bgwriter"
    };
    let row = client.query_one(sql, &[]).await?;
    let (timed, requested): (i64, i64) = (row.get(0), row.get(1));
    let (write_ms, sync_ms): (f64, f64) = (row.get(2), row.get(3));

    let wal = if caps.has_pg_stat_wal() {
        let row = client
            .query_one(
                "SELECT wal_records, wal_fpi, wal_bytes::float8, wal_buffers_full,
                        extract(epoch from (now() - stats_reset))::float8
                   FROM pg_catalog.pg_stat_wal",
                &[],
            )
            .await?;
        let bytes: f64 = row.get(2);
        let reset: Option<f64> = row.get(4);
        Some(WalReport {
            records: row.get(0),
            full_page_images: row.get(1),
            bytes,
            buffers_full: row.get(3),
            reset_seconds: reset,
            bytes_per_second: reset.filter(|s| *s > 0.0).map(|s| bytes / s),
        })
    } else {
        None
    };

    let values = settings(
        client,
        &[
            "max_wal_size",
            "checkpoint_timeout",
            "checkpoint_completion_target",
        ],
    )
    .await?;
    let max_wal_size_bytes = values.get("max_wal_size").and_then(setting_bytes);
    let completion_target = values
        .get("checkpoint_completion_target")
        .and_then(|s| s.value.as_deref())
        .and_then(|v| v.parse().ok());
    let (level, advice) =
        checkpoint_advice(timed, requested, max_wal_size_bytes, completion_target);

    Ok(CheckpointReport {
        timed,
        requested,
        forced_ratio: (timed + requested > 0)
            .then(|| requested as f64 / (timed + requested) as f64),
        write_seconds: write_ms / 1000.0,
        sync_seconds: sync_ms / 1000.0,
        buffers_written: row.get(4),
        reset_seconds: row.get(5),
        max_wal_size_bytes,
        checkpoint_timeout_seconds: values
            .get("checkpoint_timeout")
            .and_then(|s| s.value.as_deref())
            .and_then(|v| v.parse().ok()),
        completion_target,
        wal,
        level,
        advice,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_posicion_de_una_secuencia_ascendente() {
        let (ratio, left) = sequence_position(Some(1_073_741_824), 1, i64::from(i32::MAX), 1);
        assert!((ratio - 0.5).abs() < 0.001);
        assert_eq!(left, i64::from(i32::MAX) - 1_073_741_824);
    }

    #[test]
    fn una_secuencia_nunca_usada_no_gasto_nada() {
        let (ratio, left) = sequence_position(None, 1, 100, 1);
        assert_eq!(ratio, 0.0);
        assert_eq!(left, 99);
    }

    #[test]
    fn una_secuencia_descendente_cuenta_desde_el_maximo() {
        let (ratio, left) = sequence_position(Some(25), 1, 101, -1);
        assert!((ratio - 0.76).abs() < 0.001);
        assert_eq!(left, 24);
    }

    #[test]
    fn el_rango_de_un_bigint_no_desborda() {
        let (ratio, _) = sequence_position(Some(i64::MAX), 1, i64::MAX, 1);
        assert!(ratio > 0.99);
    }

    #[test]
    fn el_nivel_de_una_secuencia_usa_umbrales_altos_y_perdona_las_ciclicas() {
        assert_eq!(sequence_level(0.5, false), Level::Ok);
        assert_eq!(sequence_level(0.85, false), Level::Review);
        assert_eq!(sequence_level(0.97, false), Level::Attention);
        assert_eq!(sequence_level(0.97, true), Level::Info);
    }

    #[test]
    fn la_edad_de_las_transacciones_avisa_antes_del_failsafe() {
        assert_eq!(age_level(200_000_000), Level::Ok);
        assert_eq!(age_level(1_000_000_000), Level::Review);
        assert_eq!(age_level(1_600_000_000), Level::Attention);
    }

    fn defaults() -> AutovacuumSettings {
        AutovacuumSettings::from_values(&HashMap::new())
    }

    #[test]
    fn el_umbral_de_autovacuum_es_umbral_mas_factor_por_filas() {
        let (vacuum_at, analyze_at, status, own, missing) =
            autovacuum_plan(&defaults(), &[], 10_000.0, 500);
        assert_eq!(vacuum_at, 50.0 + 0.2 * 10_000.0);
        assert_eq!(analyze_at, 50.0 + 0.1 * 10_000.0);
        assert_eq!(status, AutovacuumStatus::Waiting);
        assert!(!own);
        assert_eq!(missing, 1550);
    }

    #[test]
    fn la_tabla_pisa_los_parametros_del_servidor() {
        let options = vec!["autovacuum_vacuum_scale_factor=0.01".to_owned()];
        let (vacuum_at, _, _, own, _) = autovacuum_plan(&defaults(), &options, 10_000.0, 0);
        assert_eq!(vacuum_at, 50.0 + 0.01 * 10_000.0);
        assert!(own);
    }

    #[test]
    fn pasado_el_umbral_le_toca_y_apagado_no() {
        let (_, _, status, _, missing) = autovacuum_plan(&defaults(), &[], 100.0, 500);
        assert_eq!(status, AutovacuumStatus::Due);
        assert_eq!(missing, 0);

        let off = vec!["autovacuum_enabled=false".to_owned()];
        let (_, _, status, _, _) = autovacuum_plan(&defaults(), &off, 100.0, 500);
        assert_eq!(status, AutovacuumStatus::Disabled);
    }

    #[test]
    fn reltuples_negativo_no_baja_el_umbral() {
        let (vacuum_at, ..) = autovacuum_plan(&defaults(), &[], -1.0, 0);
        assert_eq!(vacuum_at, 50.0);
    }

    #[test]
    fn una_tabla_que_le_toca_y_no_se_limpia_hace_un_dia_es_para_revisar() {
        let due = AutovacuumStatus::Due;
        assert_eq!(autovacuum_level(due, 900, 100.0, None), Level::Review);
        assert_eq!(
            autovacuum_level(due, 900, 100.0, Some(172_800.0)),
            Level::Review
        );
        assert_eq!(autovacuum_level(due, 900, 100.0, Some(60.0)), Level::Info);
        assert_eq!(
            autovacuum_level(AutovacuumStatus::Waiting, 1, 100.0, None),
            Level::Ok
        );
    }

    #[test]
    fn la_cache_solo_avisa_con_volumen() {
        assert_eq!(cache_level(100, 10), Level::Ok);
        assert_eq!(cache_level(50_000, 100_000), Level::Review);
        assert_eq!(cache_level(50_000, 5_000_000), Level::Ok);
        assert_eq!(hit_ratio(0, 0), None);
    }

    fn backend(app: &str, state: &str, seconds: f64) -> Backend {
        Backend {
            pid: 1,
            database: Some("app".into()),
            user: Some("web".into()),
            application_name: Some(app.into()),
            client_addr: Some("10.0.0.5".into()),
            backend_type: Some("client backend".into()),
            state: Some(state.into()),
            wait_event_type: None,
            wait_event: None,
            query: None,
            query_id: None,
            leader_pid: None,
            query_seconds: None,
            transaction_seconds: None,
            state_seconds: Some(seconds),
            blocked_by: vec![],
            is_monitor: false,
        }
    }

    #[test]
    fn agrupa_las_conexiones_por_aplicacion_y_cuenta_estados() {
        let backends = vec![
            backend("reportes", "idle", 30.0),
            backend("reportes", "idle", 900.0),
            backend("reportes", "active", 1.0),
            backend("api", "idle in transaction", 5.0),
        ];
        let groups = group_connections(&backends);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].application.as_deref(), Some("reportes"));
        assert_eq!(groups[0].total, 3);
        assert_eq!(groups[0].idle, 2);
        assert_eq!(groups[0].oldest_idle_seconds, Some(900.0));
        assert_eq!(groups[1].idle_in_transaction, 1);
    }

    #[test]
    fn las_conexiones_sin_permiso_van_a_un_solo_grupo() {
        let mut hidden = backend("", "idle", 1.0);
        hidden.application_name = None;
        hidden.user = None;
        hidden.client_addr = None;
        hidden.backend_type = None;
        hidden.state = None;
        let groups = group_connections(&[hidden.clone(), hidden]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].total, 2);
        assert!(groups[0].user.is_none());
    }

    #[test]
    fn el_monitor_y_los_procesos_internos_no_cuentan() {
        let mut monitor = backend("pgforge", "active", 0.0);
        monitor.is_monitor = true;
        let mut worker = backend("", "active", 0.0);
        worker.backend_type = Some("autovacuum worker".into());
        assert!(group_connections(&[monitor, worker]).is_empty());
    }

    #[test]
    fn escribe_tamanos_como_los_acepta_alter_system() {
        assert_eq!(pg_size(4 * (1 << 30)), "4GB");
        assert_eq!(pg_size(512 * (1 << 20)), "512MB");
        assert_eq!(pg_size(1536 * (1 << 20)), "1536MB");
        assert_eq!(human_bytes(1_610_612_736), "1,5 GB");
        assert_eq!(human_bytes(134_217_728), "128 MB");
    }

    #[test]
    fn lee_la_unidad_de_un_parametro() {
        let setting = |value: &str, unit: &str| Setting {
            value: Some(value.into()),
            unit: Some(unit.into()),
            source: "default".into(),
            boot_val: None,
            context: "user".into(),
        };
        assert_eq!(setting_bytes(&setting("16384", "8kB")), Some(134_217_728));
        assert_eq!(setting_bytes(&setting("4096", "kB")), Some(4_194_304));
        assert_eq!(setting_bytes(&setting("1024", "MB")), Some(1_073_741_824));
        assert_eq!(setting_bytes(&setting("100", "ms")), None);
    }

    #[test]
    fn los_checkpoints_forzados_piden_subir_max_wal_size() {
        let (level, advice) = checkpoint_advice(30, 70, Some(1 << 30), Some(0.9));
        assert_eq!(level, Level::Attention);
        assert!(advice[0].contains("70 %") && advice[0].contains("1 GB"));

        let (level, _) = checkpoint_advice(70, 30, None, Some(0.9));
        assert_eq!(level, Level::Review);
    }

    #[test]
    fn pocos_checkpoints_no_dicen_nada() {
        let (level, advice) = checkpoint_advice(1, 4, None, Some(0.9));
        assert_eq!(level, Level::Ok);
        assert!(advice.is_empty());
    }

    #[test]
    fn un_completion_target_bajo_es_informativo() {
        let (level, advice) = checkpoint_advice(100, 0, None, Some(0.5));
        assert_eq!(level, Level::Info);
        assert_eq!(advice.len(), 1);
    }
}
