//! Ensayo: correr un script adentro de `BEGIN … ROLLBACK` para ver qué habría pasado.
//!
//! Responde la pregunta de antes de tocar producción: cuántas filas afecta cada sentencia, cuáles
//! fallan y qué locks quedan tomados al final. Termina **siempre** en `ROLLBACK`, incluso si una
//! sentencia falla o la ejecución se cancela, así que no queda nada aplicado.
//!
//! Se parte en dos para que lo riesgoso sea lo que se puede leer sin servidor. [`blockers`] y
//! [`caveats`] son **puras**: deciden qué sentencias no pueden ensayarse (las que PostgreSQL rechaza
//! adentro de una transacción, o las que la cerrarían antes de tiempo y dejarían el resto aplicado
//! de verdad) y qué efectos no se revierten. [`rehearse`] es lo único que habla con el servidor.
//!
//! Lo que el ensayo **no** puede deshacer se avisa en vez de ocultarse: las secuencias no vuelven
//! atrás (un `INSERT` ya gastó su número), y nada de lo que salga del servidor —`dblink`, `COPY …
//! PROGRAM`, un bloqueo asesor de sesión— está en la transacción.

use std::time::Instant;

use serde::Serialize;

use super::exec::{Limits, Outcome, QuerySession, TxStatus};
use super::lex::{lex, TokenKind};
use super::split::{split, Statement};
use crate::error::{Error, ErrorPayload, Result};

/// Cuánto espera un lock ajeno antes de rendirse, por omisión.
///
/// Sin esto, ensayar un `ALTER TABLE` contra una tabla en uso se queda esperando el
/// `ACCESS EXCLUSIVE` y, mientras espera, **encola detrás de sí** a todas las consultas que vengan
/// después: el ensayo —que existe para no hacer daño— terminaría frenando la tabla.
pub const DEFAULT_LOCK_TIMEOUT_MS: u64 = 3_000;

/// Filas que se conservan por sentencia: alcanza para ver qué devolvería un `RETURNING`.
const MAX_ROWS: usize = 50;

#[derive(Debug, Clone, Copy)]
pub struct RehearsalOptions {
    /// `0` deja el `lock_timeout` de la sesión como estaba.
    pub lock_timeout_ms: u64,
}

impl Default for RehearsalOptions {
    fn default() -> Self {
        Self {
            lock_timeout_ms: DEFAULT_LOCK_TIMEOUT_MS,
        }
    }
}

/// Una sentencia que no se puede ensayar, y por qué.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocker {
    pub index: usize,
    pub line: usize,
    /// Primera línea de la sentencia, recortada.
    pub statement: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CaveatKind {
    /// Consume números de una secuencia que el `ROLLBACK` no devuelve.
    Sequence,
    /// Hace algo fuera de la base: nada de eso está en la transacción.
    External,
    /// Cambia algo de la sesión que sobrevive al `ROLLBACK`.
    Session,
}

/// Un efecto que el ensayo no revierte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Caveat {
    pub kind: CaveatKind,
    pub index: usize,
    pub message: String,
}

/// Un lock que la transacción del ensayo tenía tomado justo antes del `ROLLBACK`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockEntry {
    /// `esquema.tabla`; `None` en los que no son de una relación (consultivos, de objeto).
    pub relation: Option<String>,
    pub lock_type: String,
    pub mode: String,
    pub granted: bool,
}

/// Qué pasó con una sentencia.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub index: usize,
    pub line: usize,
    pub statement: String,
    /// `None` si falló.
    pub outcome: Option<Outcome>,
    pub error: Option<ErrorPayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Cuántas sentencias tenía el script, hayan corrido o no.
    pub total: usize,
    pub steps: Vec<Step>,
    pub locks: Vec<LockEntry>,
    /// `false` si una sentencia falló: en una transacción abortada no se puede leer `pg_locks`.
    pub locks_available: bool,
    pub caveats: Vec<Caveat>,
    /// Si no está vacío, **no se ejecutó nada**.
    pub blocked: Vec<Blocker>,
    /// Se emitió el `ROLLBACK` y el servidor lo aceptó. Con `blocked` no hay transacción: `false`.
    pub rolled_back: bool,
    pub rollback_error: Option<String>,
    pub seconds: f64,
}

/// Palabras del nivel superior de una sentencia, en mayúsculas.
///
/// Las cadenas, los identificadores citados, los cuerpos `$$ … $$` y los comentarios son tokens
/// aparte para el lexer, así que un `'VACUUM'` o un `-- CREATE DATABASE` no se confunden con
/// comandos.
fn words(sql: &str) -> Vec<String> {
    lex(sql)
        .into_iter()
        .filter(|token| token.kind == TokenKind::Word)
        .map(|token| token.text.to_uppercase())
        .collect()
}

fn preview(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("--"))
        .unwrap_or("");
    let mut shown: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        shown.push('…');
    }
    shown
}

/// Por qué una sentencia no puede ensayarse, si no puede.
fn blocker_reason(words: &[String]) -> Option<&'static str> {
    let first = words.first().map(String::as_str)?;
    let second = words.get(1).map(String::as_str);
    let has = |word: &str| words.iter().any(|w| w == word);

    const CLOSES_TX: &str = "cierra o abre una transacción: el ensayo ya corre adentro de una y, \
         si se cerrara antes de tiempo, lo que sigue quedaría aplicado de verdad";

    match first {
        "BEGIN" | "START" | "COMMIT" | "END" | "ABORT" => Some(CLOSES_TX),
        "PREPARE" if second == Some("TRANSACTION") => Some(CLOSES_TX),
        "ROLLBACK" if second != Some("TO") => Some(CLOSES_TX),
        "VACUUM" => Some("VACUUM no puede correr adentro de una transacción"),
        "CLUSTER" if words.len() == 1 => {
            Some("CLUSTER sin tabla no puede correr adentro de una transacción")
        }
        "DISCARD" => Some("DISCARD no puede correr adentro de una transacción"),
        "REINDEX" if has("CONCURRENTLY") || matches!(second, Some("DATABASE" | "SYSTEM")) => {
            Some("este REINDEX no puede correr adentro de una transacción")
        }
        "CREATE" | "DROP" if second == Some("DATABASE") => {
            Some("CREATE/DROP DATABASE no pueden correr adentro de una transacción")
        }
        "CREATE" | "DROP" if second == Some("TABLESPACE") => {
            Some("CREATE/DROP TABLESPACE no pueden correr adentro de una transacción")
        }
        "CREATE" | "DROP" if second == Some("SUBSCRIPTION") => Some(
            "las suscripciones crean o borran su slot, que no puede ir adentro de una transacción",
        ),
        "ALTER" if second == Some("SYSTEM") => {
            Some("ALTER SYSTEM no puede correr adentro de una transacción")
        }
        "ALTER" if second == Some("DATABASE") && has("TABLESPACE") => {
            Some("mover una base de tablespace no puede correr adentro de una transacción")
        }
        "CREATE" | "DROP" | "ALTER" if has("CONCURRENTLY") => {
            Some("CONCURRENTLY no puede correr adentro de una transacción")
        }
        _ => None,
    }
}

/// Las sentencias del script que impiden ensayarlo. Con una sola, no se ejecuta nada.
pub fn blockers(statements: &[Statement]) -> Vec<Blocker> {
    statements
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            blocker_reason(&words(&statement.text)).map(|reason| Blocker {
                index,
                line: statement.line,
                statement: preview(&statement.text),
                reason: reason.to_owned(),
            })
        })
        .collect()
}

/// Los efectos del script que el `ROLLBACK` no deshace. Es una lectura del texto, no una garantía:
/// lo que se esconde adentro de una función o de SQL dinámico no se ve.
pub fn caveats(statements: &[Statement]) -> Vec<Caveat> {
    let mut found = Vec::new();

    for (index, statement) in statements.iter().enumerate() {
        let words = words(&statement.text);
        let first = words.first().map(String::as_str).unwrap_or("");
        let has = |word: &str| words.iter().any(|w| w == word);
        let mut push = |kind, message: &str| {
            found.push(Caveat {
                kind,
                index,
                message: message.to_owned(),
            })
        };

        if matches!(first, "INSERT" | "COPY")
            || has("NEXTVAL")
            || has("SETVAL")
            || (first == "ALTER" && words.get(1).map(String::as_str) == Some("SEQUENCE"))
        {
            push(
                CaveatKind::Sequence,
                "usa secuencias: los números consumidos no se devuelven con el ROLLBACK",
            );
        }

        if has("DBLINK")
            || words.iter().any(|w| w.starts_with("DBLINK_"))
            || (first == "COPY" && has("PROGRAM"))
            || has("LO_EXPORT")
            || has("PG_TERMINATE_BACKEND")
            || has("PG_CANCEL_BACKEND")
            || has("PG_RELOAD_CONF")
        {
            push(
                CaveatKind::External,
                "actúa fuera de la transacción (otra conexión, un programa, un archivo o un \
                 proceso del servidor): el ROLLBACK no lo deshace",
            );
        }

        if has("PG_ADVISORY_LOCK") || has("PG_TRY_ADVISORY_LOCK") {
            push(
                CaveatKind::Session,
                "toma un bloqueo consultivo de sesión: sobrevive al ROLLBACK hasta que se libere \
                 o se cierre la pestaña",
            );
        }
    }

    found
}

/// Ensaya el script en la sesión dada.
///
/// Exige que la sesión no tenga una transacción abierta: el ensayo abre y cierra la suya, y
/// anidarla dentro de la del usuario terminaría revirtiendo trabajo que no era del ensayo.
pub async fn rehearse(
    session: &QuerySession,
    sql: &str,
    options: RehearsalOptions,
) -> Result<Report> {
    let statements = split(sql);
    if statements.is_empty() {
        return Err(Error::Config(
            "no hay ninguna sentencia para ensayar".to_owned(),
        ));
    }

    let blocked = blockers(&statements);
    let caveats = caveats(&statements);

    if !blocked.is_empty() {
        return Ok(Report {
            total: statements.len(),
            steps: Vec::new(),
            locks: Vec::new(),
            locks_available: false,
            caveats,
            blocked,
            rolled_back: false,
            rollback_error: None,
            seconds: 0.0,
        });
    }

    if session.tx_status().await? != TxStatus::Idle {
        return Err(Error::Config(
            "hay una transacción abierta en esta pestaña: confirmala o revertila antes de ensayar"
                .to_owned(),
        ));
    }

    let started = Instant::now();
    session.run("BEGIN", Limits::default()).await?;

    let inside = run_inside(session, &statements, options).await;

    // El ROLLBACK va pase lo que pase con el cuerpo: es la promesa del ensayo.
    let rollback = session.rollback().await;
    let (steps, locks, locks_available) = inside?;

    Ok(Report {
        total: statements.len(),
        steps,
        locks,
        locks_available,
        caveats,
        blocked,
        rolled_back: rollback.is_ok(),
        rollback_error: rollback.err().map(|error| error.to_string()),
        seconds: started.elapsed().as_secs_f64(),
    })
}

async fn run_inside(
    session: &QuerySession,
    statements: &[Statement],
    options: RehearsalOptions,
) -> Result<(Vec<Step>, Vec<LockEntry>, bool)> {
    if options.lock_timeout_ms > 0 {
        session
            .run(
                &format!("SET LOCAL lock_timeout = {}", options.lock_timeout_ms),
                Limits::default(),
            )
            .await?;
    }

    let limits = Limits { max_rows: MAX_ROWS };
    let mut steps = Vec::new();

    for (index, statement) in statements.iter().enumerate() {
        let (outcome, error) = match session.run(&statement.text, limits).await {
            Ok(outcome) => (Some(outcome), None),
            Err(error @ (Error::Connection(_) | Error::ConnectionClosed)) => return Err(error),
            Err(error) => (None, Some(ErrorPayload::from(&error))),
        };
        let failed = error.is_some();

        steps.push(Step {
            index,
            line: statement.line,
            statement: preview(&statement.text),
            outcome,
            error,
        });

        // Después de un error la transacción está abortada: lo que sigue fallaría igual y solo
        // taparía la causa.
        if failed {
            return Ok((steps, Vec::new(), false));
        }
    }

    let locks = read_locks(session).await?;
    Ok((steps, locks, true))
}

/// Los locks de esta sesión. `pg_locks` existe igual en todo el rango soportado.
///
/// Se descartan los de transacción y de `virtualxid`, que toda transacción tiene y no dicen nada, y
/// los de índices y toast, que acompañan a cada lock de su tabla y multiplican las filas sin
/// agregar información.
const LOCKS_SQL: &str = "\
SELECT l.locktype, l.mode, l.granted::text, n.nspname, c.relname \
FROM pg_locks l \
LEFT JOIN pg_class c ON l.locktype = 'relation' AND c.oid = l.relation \
LEFT JOIN pg_namespace n ON n.oid = c.relnamespace \
WHERE l.pid = pg_backend_pid() \
  AND l.locktype NOT IN ('virtualxid', 'transactionid') \
  AND (c.oid IS NULL OR (c.relkind NOT IN ('i', 'I', 't') \
       AND n.nspname NOT IN ('pg_catalog', 'information_schema'))) \
ORDER BY n.nspname, c.relname, l.mode";

async fn read_locks(session: &QuerySession) -> Result<Vec<LockEntry>> {
    let Outcome::Rows { rows, .. } = session.run(LOCKS_SQL, Limits { max_rows: 1_000 }).await?
    else {
        return Ok(Vec::new());
    };

    Ok(rows
        .into_iter()
        .map(|row| {
            let cell = |index: usize| row.get(index).cloned().flatten();
            let relation = match (cell(3), cell(4)) {
                (Some(schema), Some(name)) => Some(format!("{schema}.{name}")),
                _ => None,
            };
            LockEntry {
                relation,
                lock_type: cell(0).unwrap_or_default(),
                mode: cell(1).unwrap_or_default(),
                granted: cell(2).as_deref() == Some("true"),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reasons(sql: &str) -> Vec<String> {
        blockers(&split(sql))
            .into_iter()
            .map(|b| b.reason)
            .collect()
    }

    fn blocked(sql: &str) -> bool {
        !reasons(sql).is_empty()
    }

    #[test]
    fn rechaza_lo_que_postgresql_no_deja_correr_en_transaccion() {
        for sql in [
            "VACUUM clientes",
            "vacuum (analyze)",
            "CREATE INDEX CONCURRENTLY ix ON t (a)",
            "create unique index concurrently ix on t (a)",
            "DROP INDEX CONCURRENTLY ix",
            "REINDEX INDEX CONCURRENTLY ix",
            "REINDEX DATABASE d",
            "CREATE DATABASE nueva",
            "drop database vieja",
            "CREATE TABLESPACE ts LOCATION '/x'",
            "ALTER SYSTEM SET work_mem = '1GB'",
            "ALTER TABLE t DETACH PARTITION p CONCURRENTLY",
            "CLUSTER",
            "DISCARD ALL",
        ] {
            assert!(blocked(sql), "debía rechazarse: {sql}");
        }
    }

    #[test]
    fn rechaza_lo_que_cerraria_la_transaccion_del_ensayo() {
        for sql in [
            "COMMIT",
            "END",
            "ABORT",
            "ROLLBACK",
            "BEGIN",
            "START TRANSACTION",
            "PREPARE TRANSACTION 'x'",
        ] {
            assert!(blocked(sql), "debía rechazarse: {sql}");
        }
    }

    #[test]
    fn deja_pasar_lo_que_si_corre_en_transaccion() {
        for sql in [
            "SELECT 1",
            "INSERT INTO t VALUES (1)",
            "ALTER TABLE t ADD COLUMN c int",
            "CREATE INDEX ix ON t (a)",
            "REFRESH MATERIALIZED VIEW CONCURRENTLY v",
            "ANALYZE t",
            "ROLLBACK TO SAVEPOINT s",
            "SAVEPOINT s",
            "CLUSTER t USING ix",
            "PREPARE p AS SELECT 1",
        ] {
            assert!(!blocked(sql), "no debía rechazarse: {sql}");
        }
    }

    #[test]
    fn no_confunde_un_comando_con_texto_de_una_cadena_o_un_comentario() {
        assert!(!blocked("SELECT 'VACUUM'"));
        assert!(!blocked("-- VACUUM t\nSELECT 1"));
        assert!(!blocked("/* CREATE DATABASE x */ SELECT 1"));
        assert!(!blocked("DO $$ BEGIN PERFORM 1; END $$"));
        assert!(!blocked("SELECT \"commit\" FROM t"));
    }

    #[test]
    fn senala_cual_sentencia_y_en_que_linea() {
        let found = blockers(&split("SELECT 1;\n\nVACUUM t;"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].index, 1);
        assert_eq!(found[0].line, 3);
        assert_eq!(found[0].statement, "VACUUM t");
    }

    #[test]
    fn avisa_de_las_secuencias_y_de_los_efectos_externos() {
        let kinds = |sql: &str| -> Vec<CaveatKind> {
            caveats(&split(sql)).into_iter().map(|c| c.kind).collect()
        };

        assert_eq!(kinds("INSERT INTO t VALUES (1)"), [CaveatKind::Sequence]);
        assert_eq!(kinds("SELECT nextval('s')"), [CaveatKind::Sequence]);
        assert_eq!(
            kinds("SELECT dblink_exec('x', 'DELETE FROM t')"),
            [CaveatKind::External]
        );
        assert_eq!(kinds("COPY t FROM PROGRAM 'ls'").len(), 2);
        assert_eq!(kinds("SELECT pg_advisory_lock(1)"), [CaveatKind::Session]);
        assert!(kinds("UPDATE t SET a = 1").is_empty());
    }
}
