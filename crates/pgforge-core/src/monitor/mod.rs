//! Monitoreo y mantenimiento.
//!
//! El monitoreo usa una conexión propia, fuera del pool del explorador. Refrescar cada dos
//! segundos sobre las mismas conexiones que usa el resto de la aplicación haría que el árbol de
//! objetos compita con el dashboard justo cuando el servidor está en problemas, que es cuando uno
//! abre el dashboard.
//!
//! El intervalo de refresco no vive acá: lo decide quien consume estos datos, que es el único que
//! sabe si su ventana está visible. Este módulo solo sabe tomar una muestra.

pub mod activity;
pub mod maintenance;
pub mod stats;

pub use activity::{ActivityFilter, Backend, BlockNode, Lock};
pub use maintenance::{Operation, Target};
pub use stats::{IndexStat, StatementStat, TableStat};

use std::time::Instant;

use serde::Serialize;

use crate::caps::ServerCaps;
use crate::conn::{ServerHandle, Session};
use crate::error::{Error, Result};

/// Una consulta de monitoreo que tarda esto está rota. Cortarla evita que una muestra trabada
/// deje al dashboard congelado sin explicación.
const MONITOR_STATEMENT_TIMEOUT_MS: u64 = 15_000;

/// Indicadores del servidor en el momento de la muestra.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub total_connections: i64,
    pub active_connections: i64,
    /// Sesiones con una transacción abierta sin hacer nada. Retienen candados e impiden que el
    /// vacuum limpie versiones viejas de las filas.
    pub idle_in_transaction: i64,
    pub waiting_connections: i64,
    pub max_connections: i64,
    /// Transacciones por segundo desde la muestra anterior. Es `None` en la primera.
    pub transactions_per_second: Option<f64>,
    pub cache_hit_ratio: Option<f64>,
    pub longest_transaction_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub backends: Vec<Backend>,
    pub blocking: Vec<BlockNode>,
    pub metrics: Metrics,
}

const SERVER_METRICS_SQL: &str = "
    SELECT (SELECT setting::int8 FROM pg_catalog.pg_settings WHERE name = 'max_connections'),
           (SELECT sum(xact_commit + xact_rollback)::int8 FROM pg_catalog.pg_stat_database),
           (SELECT sum(blks_hit)::float8 / nullif(sum(blks_hit + blks_read), 0)
              FROM pg_catalog.pg_stat_database)
";

/// Sesión de monitoreo de un servidor.
pub struct Monitor {
    session: Session,
    caps: ServerCaps,
    /// Momento y total de transacciones de la muestra anterior, para derivar la tasa.
    previous_transactions: Option<(Instant, i64)>,
}

impl Monitor {
    pub async fn open(handle: &ServerHandle, database: &str) -> Result<Self> {
        let session = handle
            .open_session(database, Some(MONITOR_STATEMENT_TIMEOUT_MS))
            .await?;

        Ok(Self {
            session,
            caps: handle.caps.clone(),
            previous_transactions: None,
        })
    }

    pub fn caps(&self) -> &ServerCaps {
        &self.caps
    }

    pub async fn snapshot(&mut self, filter: &ActivityFilter) -> Result<Snapshot> {
        let client = self.session.client();
        let all = activity::backends(client, &self.caps).await?;
        let row = client.query_one(SERVER_METRICS_SQL, &[]).await?;

        let max_connections: i64 = row.get::<_, Option<i64>>(0).unwrap_or(0);
        let transactions: Option<i64> = row.get(1);
        let cache_hit_ratio: Option<f64> = row.get(2);

        let now = Instant::now();
        let transactions_per_second = match (self.previous_transactions, transactions) {
            (Some((then, before)), Some(current)) => {
                let elapsed = now.duration_since(then).as_secs_f64();
                // Si las estadísticas se reiniciaron, el contador baja y la tasa daría negativa.
                (elapsed > 0.0 && current >= before).then(|| (current - before) as f64 / elapsed)
            }
            _ => None,
        };
        if let Some(current) = transactions {
            self.previous_transactions = Some((now, current));
        }

        let metrics = Metrics {
            total_connections: all.iter().filter(|b| !b.is_background()).count() as i64,
            active_connections: all
                .iter()
                .filter(|b| b.state.as_deref() == Some("active"))
                .count() as i64,
            idle_in_transaction: all
                .iter()
                .filter(|b| {
                    matches!(
                        b.state.as_deref(),
                        Some("idle in transaction") | Some("idle in transaction (aborted)")
                    )
                })
                .count() as i64,
            waiting_connections: all.iter().filter(|b| !b.blocked_by.is_empty()).count() as i64,
            max_connections,
            transactions_per_second,
            cache_hit_ratio,
            longest_transaction_seconds: all
                .iter()
                .filter(|b| !b.is_background())
                .filter_map(|b| b.transaction_seconds)
                .max_by(|a, b| a.total_cmp(b)),
        };

        // El árbol de bloqueo se arma sobre todas las sesiones: una sesión inactiva que el filtro
        // oculta puede ser justamente la que está bloqueando al resto.
        let blocking = activity::blocking_tree(&all);

        Ok(Snapshot {
            backends: filter.apply(all),
            blocking,
            metrics,
        })
    }

    pub async fn locks(&self, pid: i32) -> Result<Vec<Lock>> {
        activity::locks(self.session.client(), pid).await
    }

    /// Pide al servidor que aborte la consulta en curso de una sesión, dejándola conectada.
    pub async fn cancel_backend(&self, pid: i32) -> Result<bool> {
        self.signal("pg_cancel_backend", pid).await
    }

    /// Cierra la sesión entera. Es más agresivo que cancelar: la transacción abierta se revierte y
    /// el cliente pierde la conexión.
    pub async fn terminate_backend(&self, pid: i32) -> Result<bool> {
        self.signal("pg_terminate_backend", pid).await
    }

    async fn signal(&self, function: &str, pid: i32) -> Result<bool> {
        let sql = format!("SELECT pg_catalog.{function}($1)");
        match self.session.client().query_one(sql.as_str(), &[&pid]).await {
            Ok(row) => Ok(row.get(0)),
            Err(err) => {
                let error = Error::from(err);
                // El servidor responde "permission denied" a secas. Decir cuál es el permiso que
                // falta es la diferencia entre un error y algo que el usuario puede resolver.
                if matches!(error, Error::Permission(_)) && !self.caps.can_signal_backends {
                    return Err(Error::Permission(format!(
                        "para actuar sobre sesiones de otros usuarios hay que ser superusuario o \
                         miembro del rol pg_signal_backend, y {} no lo es",
                        self.caps.current_user
                    )));
                }
                Err(error)
            }
        }
    }

    pub async fn tables(&self, limit: i64) -> Result<Vec<TableStat>> {
        self.require_stats()?;
        stats::tables(self.session.client(), limit).await
    }

    pub async fn indexes(&self, limit: i64) -> Result<Vec<IndexStat>> {
        self.require_stats()?;
        stats::indexes(self.session.client(), limit).await
    }

    pub async fn has_statement_stats(&self) -> Result<bool> {
        stats::has_statement_stats(self.session.client()).await
    }

    pub async fn statements(&self, limit: i64) -> Result<Vec<StatementStat>> {
        if !self.has_statement_stats().await? {
            return Err(Error::Config(
                "la extensión pg_stat_statements no está instalada en esta base. Se instala con \
                 CREATE EXTENSION pg_stat_statements, y requiere que la biblioteca esté cargada \
                 en shared_preload_libraries."
                    .to_owned(),
            ));
        }
        stats::statements(self.session.client(), limit).await
    }

    /// Sin `pg_read_all_stats` las vistas de estadísticas devuelven filas vacías en lugar de un
    /// error, y una tabla vacía sin explicación parece un problema de la aplicación.
    fn require_stats(&self) -> Result<()> {
        if !self.caps.can_read_all_stats && !self.caps.is_superuser {
            tracing::debug!(
                usuario = %self.caps.current_user,
                "sin pg_read_all_stats: las estadísticas pueden venir incompletas"
            );
        }
        Ok(())
    }
}
