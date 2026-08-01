//! Sesiones activas y bloqueos.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tokio_postgres::Client;

use crate::caps::ServerCaps;
use crate::error::Result;

/// Una sesión del servidor, tal como la ve `pg_stat_activity`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backend {
    pub pid: i32,
    pub database: Option<String>,
    pub user: Option<String>,
    pub application_name: String,
    pub client_addr: Option<String>,
    pub backend_type: String,
    pub state: Option<String>,
    pub wait_event_type: Option<String>,
    pub wait_event: Option<String>,
    pub query: Option<String>,
    /// Identificador de la consulta normalizada. Requiere PostgreSQL 14.
    pub query_id: Option<i64>,
    /// Sesión líder cuando este backend es un worker paralelo.
    pub leader_pid: Option<i32>,
    /// Segundos desde que empezó la consulta actual.
    pub query_seconds: Option<f64>,
    /// Segundos desde que empezó la transacción actual.
    pub transaction_seconds: Option<f64>,
    /// Segundos en el estado actual. Un `idle in transaction` largo retiene recursos y bloquea el
    /// avance del horizonte de vacuum, así que conviene tenerlo a la vista.
    pub state_seconds: Option<f64>,
    /// PIDs que impiden avanzar a esta sesión, según `pg_blocking_pids`.
    pub blocked_by: Vec<i32>,
    /// La sesión del propio monitor, que no tiene sentido cancelar.
    pub is_monitor: bool,
}

impl Backend {
    pub fn is_idle(&self) -> bool {
        self.state.as_deref() == Some("idle")
    }

    /// Los procesos internos del servidor (writer, checkpointer, autovacuum) no son sesiones de
    /// usuario y en la lista solo agregan ruido.
    pub fn is_background(&self) -> bool {
        self.backend_type != "client backend"
    }
}

/// Qué sesiones incluir en la vista.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityFilter {
    pub include_idle: bool,
    pub include_background: bool,
    pub database: Option<String>,
}

impl ActivityFilter {
    /// Aplica el filtro. Se hace en memoria y no en la consulta para que las métricas del servidor
    /// se calculen siempre sobre todas las sesiones: si el usuario oculta las inactivas, el total
    /// de conexiones no tiene que cambiar.
    pub fn apply(&self, backends: Vec<Backend>) -> Vec<Backend> {
        backends
            .into_iter()
            .filter(|backend| self.keeps(backend))
            .collect()
    }

    fn keeps(&self, backend: &Backend) -> bool {
        if !self.include_idle && backend.is_idle() {
            return false;
        }
        if !self.include_background && backend.is_background() {
            return false;
        }
        match (&self.database, &backend.database) {
            (Some(wanted), Some(actual)) => wanted == actual,
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

/// Arma la consulta según lo que ofrezca el servidor.
///
/// `query_id` existe desde PostgreSQL 14 y `leader_pid` desde la 13; pedirlas contra una versión
/// anterior no devuelve nulos, hace fallar la consulta entera.
fn activity_sql(caps: &ServerCaps) -> String {
    let query_id = if caps.has_query_id() {
        "a.query_id"
    } else {
        "NULL::int8"
    };
    let leader_pid = if caps.has_leader_pid() {
        "a.leader_pid"
    } else {
        "NULL::int4"
    };

    format!(
        "SELECT a.pid,
                a.datname::text,
                a.usename::text,
                a.application_name,
                host(a.client_addr),
                a.backend_type,
                a.state,
                a.wait_event_type,
                a.wait_event,
                a.query,
                {query_id},
                {leader_pid},
                extract(epoch from (now() - a.query_start))::float8,
                extract(epoch from (now() - a.xact_start))::float8,
                extract(epoch from (now() - a.state_change))::float8,
                pg_catalog.pg_blocking_pids(a.pid),
                a.pid = pg_catalog.pg_backend_pid()
           FROM pg_catalog.pg_stat_activity a
          ORDER BY a.backend_start"
    )
}

/// Trae todas las sesiones del servidor, sin filtrar.
///
/// La cantidad de filas está acotada por `max_connections`, así que traerlas todas y filtrar del
/// lado del cliente evita ir al servidor cada vez que alguien destilda una casilla.
pub async fn backends(client: &Client, caps: &ServerCaps) -> Result<Vec<Backend>> {
    let rows = client.query(activity_sql(caps).as_str(), &[]).await?;

    Ok(rows
        .into_iter()
        .map(|row| Backend {
            pid: row.get(0),
            database: row.get(1),
            user: row.get(2),
            application_name: row.get(3),
            client_addr: row.get(4),
            backend_type: row.get(5),
            state: row.get(6),
            wait_event_type: row.get(7),
            wait_event: row.get(8),
            query: row.get(9),
            query_id: row.get(10),
            leader_pid: row.get(11),
            query_seconds: row.get(12),
            transaction_seconds: row.get(13),
            state_seconds: row.get(14),
            blocked_by: row.get(15),
            is_monitor: row.get(16),
        })
        .collect())
}

/// Un nodo del árbol de bloqueo: la sesión, y las que están esperando por ella.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockNode {
    pub pid: i32,
    pub blocking: Vec<BlockNode>,
}

/// Arma el árbol de bloqueo a partir de `pg_blocking_pids`, que ya viene en cada sesión.
///
/// Reconstruirlo cruzando `pg_locks` consigo mismo es la forma habitual de hacerlo y es fácil de
/// equivocar; la función del servidor ya resuelve los casos difíciles, incluidos los bloqueos por
/// tupla y los workers paralelos.
pub fn blocking_tree(backends: &[Backend]) -> Vec<BlockNode> {
    let mut waiting_on: HashMap<i32, Vec<i32>> = HashMap::new();
    let mut blocked: HashSet<i32> = HashSet::new();
    let mut involved: HashSet<i32> = HashSet::new();

    for backend in backends {
        for blocker in &backend.blocked_by {
            waiting_on.entry(*blocker).or_default().push(backend.pid);
            blocked.insert(backend.pid);
            involved.insert(*blocker);
            involved.insert(backend.pid);
        }
    }

    let mut roots: Vec<i32> = involved
        .into_iter()
        .filter(|pid| !blocked.contains(pid))
        .collect();
    roots.sort_unstable();

    roots
        .into_iter()
        .map(|pid| build_node(pid, &waiting_on, &mut HashSet::new()))
        .collect()
}

fn build_node(pid: i32, waiting_on: &HashMap<i32, Vec<i32>>, seen: &mut HashSet<i32>) -> BlockNode {
    // Un ciclo de espera no debería existir —el servidor detecta los interbloqueos y aborta una de
    // las transacciones—, pero entre dos muestras se puede ver uno a medio resolver, y recorrerlo
    // sin marca de visitados sería recursión infinita.
    if !seen.insert(pid) {
        return BlockNode {
            pid,
            blocking: Vec::new(),
        };
    }

    let blocking = waiting_on
        .get(&pid)
        .map(|children| {
            children
                .iter()
                .map(|child| build_node(*child, waiting_on, seen))
                .collect()
        })
        .unwrap_or_default();

    BlockNode { pid, blocking }
}

/// Detalle de los candados que tiene o espera una sesión.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    pub lock_type: String,
    pub relation: Option<String>,
    pub mode: String,
    pub granted: bool,
}

pub async fn locks(client: &Client, pid: i32) -> Result<Vec<Lock>> {
    let rows = client
        .query(
            "SELECT l.locktype,
                    CASE WHEN l.relation IS NOT NULL
                         THEN l.relation::regclass::text END,
                    l.mode,
                    l.granted
               FROM pg_catalog.pg_locks l
              WHERE l.pid = $1
              ORDER BY l.granted, l.locktype",
            &[&pid],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| Lock {
            lock_type: row.get(0),
            relation: row.get(1),
            mode: row.get(2),
            granted: row.get(3),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(pid: i32, blocked_by: Vec<i32>) -> Backend {
        Backend {
            pid,
            database: Some("app".into()),
            user: Some("app".into()),
            application_name: String::new(),
            client_addr: None,
            backend_type: "client backend".into(),
            state: Some("active".into()),
            wait_event_type: None,
            wait_event: None,
            query: None,
            query_id: None,
            leader_pid: None,
            query_seconds: None,
            transaction_seconds: None,
            state_seconds: None,
            blocked_by,
            is_monitor: false,
        }
    }

    #[test]
    fn arma_la_cadena_de_bloqueo() {
        // 10 bloquea a 11, que a su vez bloquea a 12.
        let backends = vec![
            backend(10, vec![]),
            backend(11, vec![10]),
            backend(12, vec![11]),
            backend(13, vec![]),
        ];

        let tree = blocking_tree(&backends);
        assert_eq!(tree.len(), 1, "solo 10 es raíz; 13 no participa");
        assert_eq!(tree[0].pid, 10);
        assert_eq!(tree[0].blocking[0].pid, 11);
        assert_eq!(tree[0].blocking[0].blocking[0].pid, 12);
    }

    #[test]
    fn un_ciclo_no_cuelga_el_recorrido() {
        let backends = vec![backend(20, vec![21]), backend(21, vec![20])];
        // Sin raíces no hay nada que mostrar, pero tampoco puede colgarse.
        assert!(blocking_tree(&backends).is_empty());
    }

    #[test]
    fn el_filtro_descarta_inactivas_y_procesos_internos() {
        let mut idle = backend(30, vec![]);
        idle.state = Some("idle".into());
        let mut interno = backend(31, vec![]);
        interno.backend_type = "autovacuum launcher".into();
        let activa = backend(32, vec![]);

        let filtro = ActivityFilter::default();
        assert!(!filtro.keeps(&idle));
        assert!(!filtro.keeps(&interno));
        assert!(filtro.keeps(&activa));

        let todo = ActivityFilter {
            include_idle: true,
            include_background: true,
            database: None,
        };
        assert!(todo.keeps(&idle));
        assert!(todo.keeps(&interno));
    }

    #[test]
    fn la_consulta_omite_las_columnas_que_la_version_no_tiene() {
        let caps = |num| ServerCaps {
            version: crate::ServerVersion::from_num(num),
            current_user: "postgres".into(),
            current_database: "postgres".into(),
            is_superuser: true,
            can_signal_backends: true,
            can_read_all_stats: true,
        };

        assert!(activity_sql(&caps(130_000)).contains("NULL::int8"));
        assert!(!activity_sql(&caps(130_000)).contains("a.query_id"));
        assert!(activity_sql(&caps(140_000)).contains("a.query_id"));
    }
}
