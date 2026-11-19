//! Qué está esperando una consulta que tarda.
//!
//! Una consulta de un minuto puede estar trabajando o puede estar parada detrás de otra sesión que
//! tiene una fila bloqueada, y desde la pestaña las dos se ven igual. Se le pregunta al servidor por
//! otra conexión —la de la pestaña está ocupada ejecutando— con el pid de su sesión.
//!
//! Lo que se ve de las sesiones ajenas depende del rol: sin `pg_read_all_stats`, `pg_stat_activity`
//! trae las demás filas con casi todo en NULL, así que cada dato es `Option` y la pregunta no falla
//! por falta de permisos (ver `tests/monitor_permisos.rs`). `pg_blocking_pids` existe en todo el
//! rango soportado, así que no hay predicado en `ServerCaps`.

use serde::Serialize;

use crate::conn::ServerHandle;
use crate::error::Result;

/// La sesión que tiene parada a otra.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocker {
    pub pid: i32,
    pub user: Option<String>,
    pub state: Option<String>,
    /// Hace cuántos segundos abrió su transacción, si se sabe.
    pub transaction_seconds: Option<f64>,
    /// Lo último que ejecutó, recortado: alcanza para reconocerla.
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitInfo {
    pub wait_event_type: Option<String>,
    pub wait_event: Option<String>,
    /// Cuántas sesiones la tienen parada. Casi siempre una.
    pub blocked_by: usize,
    pub blocker: Option<Blocker>,
}

const WAIT_SQL: &str = "
    SELECT wait_event_type, wait_event, pg_catalog.pg_blocking_pids(pid)
      FROM pg_catalog.pg_stat_activity
     WHERE pid = $1
";

const BLOCKER_SQL: &str = "
    SELECT usename::text,
           state,
           EXTRACT(EPOCH FROM (now() - xact_start))::float8,
           left(query, 300)
      FROM pg_catalog.pg_stat_activity
     WHERE pid = $1
";

/// Qué espera la sesión `pid`. `None` si ya no existe: la consulta terminó entre dos preguntas.
pub async fn wait_info(
    handle: &ServerHandle,
    database: &str,
    pid: i32,
) -> Result<Option<WaitInfo>> {
    let client = handle.client(database).await?;

    let Some(row) = client.query_opt(WAIT_SQL, &[&pid]).await? else {
        return Ok(None);
    };
    let blockers: Vec<i32> = row.get(2);

    let blocker = match blockers.first() {
        Some(first) => client
            .query_opt(BLOCKER_SQL, &[first])
            .await?
            .map(|row| Blocker {
                pid: *first,
                user: row.get(0),
                state: row.get(1),
                transaction_seconds: row.get(2),
                query: row.get(3),
            }),
        None => None,
    };

    Ok(Some(WaitInfo {
        wait_event_type: row.get(0),
        wait_event: row.get(1),
        blocked_by: blockers.len(),
        blocker,
    }))
}
