//! Ejecución de una sentencia y recolección de su resultado.
//!
//! Se usa el protocolo simple (`simple_query_raw`) y no el extendido. El extendido devuelve los
//! valores en binario, y decodificarlos exige un `FromSql` por cada tipo: un enum, un compuesto, un
//! dominio o cualquier tipo traído por una extensión no se podrían mostrar. Con el protocolo simple
//! el que formatea es el servidor, con la misma función de salida que usa `psql`, así que cualquier
//! tipo que exista en la base se puede mostrar sin conocerlo.
//!
//! Lo que se pierde es el tipo de cada columna, que serviría para alinear los números a la derecha.
//! Es un precio menor y se paga en la etapa de la grilla editable, que necesita esos metadatos por
//! otros motivos.

use std::time::Instant;

use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_postgres::{CancelToken, SimpleQueryMessage};

use crate::conn::{Notice, ServerHandle, Session};
use crate::error::{Error, Result};

/// Techo de filas que se guardan en memoria por omisión.
///
/// Un `SELECT` sin `WHERE` sobre una tabla de millones de filas no puede tumbar la aplicación, y
/// nadie lee diez mil filas en una grilla: las que sobran se descartan y se avisa.
pub const DEFAULT_MAX_ROWS: usize = 10_000;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub max_rows: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_rows: DEFAULT_MAX_ROWS,
        }
    }
}

/// Resultado de una sentencia.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Outcome {
    /// Devolvió filas.
    #[serde(rename_all = "camelCase")]
    Rows {
        columns: Vec<String>,
        /// `None` es un `NULL` de verdad, distinto de la cadena vacía.
        rows: Vec<Vec<Option<String>>>,
        /// Cuántas filas produjo la consulta, que puede ser más de las que se guardaron.
        row_count: u64,
        truncated: bool,
        seconds: f64,
    },
    /// No devolvió filas: un `INSERT`, un `CREATE`, un `SET`.
    #[serde(rename_all = "camelCase")]
    Command {
        tag: String,
        affected: u64,
        seconds: f64,
    },
}

/// Estado de la transacción de una pestaña de consulta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TxStatus {
    /// Sin transacción abierta: cada sentencia se confirma sola.
    Idle,
    /// Hay una transacción en curso, con cambios que todavía nadie confirmó.
    Active,
    /// Una sentencia falló dentro de la transacción: el servidor rechaza todo hasta el `ROLLBACK`.
    Failed,
}

/// Sonda con la que se averigua si hay una transacción abierta.
///
/// `now()` es el instante en que empezó la transacción y `statement_timestamp()` el de la sentencia
/// que corre. Si la transacción se abrió en una ejecución anterior, difieren; en autocommit la sonda
/// es la primera sentencia de su propia transacción y coinciden. Y si la transacción está abortada,
/// la sonda falla con `25P02`, que es justamente el tercer estado.
///
/// Parece un rodeo porque lo es: el estado viaja en cada `ReadyForQuery` del protocolo, pero
/// `tokio-postgres` descarta ese byte y no lo expone. Mientras siga siendo así, esta es la forma de
/// saberlo sin llevar la cuenta a mano leyendo el SQL del usuario, que se rompe con el primer
/// `SAVEPOINT` o el primer bloque `DO`.
const TX_PROBE: &str = "SELECT now() <> statement_timestamp()";

/// `in_failed_sql_transaction`: la transacción está abierta pero abortada.
const IN_FAILED_SQL_TRANSACTION: &str = "25P02";

/// Conexión propia de una pestaña de consulta.
///
/// Vive mientras la pestaña está abierta, no mientras dura una ejecución: es lo que hace que un
/// `BEGIN`, un `SET search_path` o una tabla temporal sigan valiendo en la consulta siguiente, que
/// es como se espera que funcione un editor de SQL.
pub struct QuerySession {
    session: Session,
}

impl QuerySession {
    pub async fn open(handle: &ServerHandle, database: &str) -> Result<Self> {
        // Con el `statement_timeout` del perfil, al revés que el mantenimiento: una consulta
        // escrita a mano es justamente lo que ese límite existe para acotar.
        let session = handle
            .open_session(database, handle.profile.statement_timeout_ms)
            .await?;

        Ok(Self { session })
    }

    /// Permite abortar desde afuera lo que esta sesión esté ejecutando.
    pub fn cancel_token(&self) -> CancelToken {
        self.session.cancel_token()
    }

    /// Canal por donde llegan los `RAISE NOTICE` del servidor. Solo el primer llamado lo obtiene.
    pub fn take_notices(&mut self) -> Option<mpsc::UnboundedReceiver<Notice>> {
        self.session.take_notices()
    }

    /// Ejecuta una sentencia y espera su resultado completo.
    pub async fn run(&self, sql: &str, limits: Limits) -> Result<Outcome> {
        let started = Instant::now();
        let stream = self.session.client().simple_query_raw(sql).await?;
        tokio::pin!(stream);

        let mut columns = Vec::new();
        let mut rows: Vec<Vec<Option<String>>> = Vec::new();
        let mut row_count = 0;
        let mut affected = 0;
        let mut returns_rows = false;

        while let Some(message) = stream.try_next().await? {
            match message {
                SimpleQueryMessage::RowDescription(description) => {
                    // Que haya descripción de columnas ya distingue un `SELECT` sin resultados de
                    // un `UPDATE`: el primero muestra una grilla vacía, el segundo un mensaje.
                    returns_rows = true;
                    columns = description
                        .iter()
                        .map(|column| column.name().to_owned())
                        .collect();
                }

                SimpleQueryMessage::Row(row) => {
                    row_count += 1;
                    // Pasado el techo se sigue consumiendo el stream sin guardar nada: soltarlo a
                    // mitad dejaría la conexión con datos pendientes, y acumular todo es lo que se
                    // está tratando de evitar.
                    if rows.len() < limits.max_rows {
                        rows.push(
                            (0..row.len())
                                .map(|index| row.get(index).map(str::to_owned))
                                .collect(),
                        );
                    }
                }

                SimpleQueryMessage::CommandComplete(count) => affected = count,

                _ => {}
            }
        }

        let seconds = started.elapsed().as_secs_f64();

        Ok(if returns_rows {
            Outcome::Rows {
                columns,
                truncated: row_count > rows.len() as u64,
                rows,
                row_count,
                seconds,
            }
        } else {
            Outcome::Command {
                tag: command_tag(sql),
                affected,
                seconds,
            }
        })
    }

    /// Si hay o no una transacción abierta en esta sesión, y si está sana.
    pub async fn tx_status(&self) -> Result<TxStatus> {
        match self.session.client().simple_query(TX_PROBE).await {
            Ok(messages) => {
                let open = messages
                    .iter()
                    .find_map(|message| match message {
                        SimpleQueryMessage::Row(row) => Some(row.get(0) == Some("t")),
                        _ => None,
                    })
                    .unwrap_or(false);

                Ok(if open {
                    TxStatus::Active
                } else {
                    TxStatus::Idle
                })
            }
            Err(error) => {
                let error = Error::from(error);
                match &error {
                    Error::Database { code, .. } if code == IN_FAILED_SQL_TRANSACTION => {
                        Ok(TxStatus::Failed)
                    }
                    _ => Err(error),
                }
            }
        }
    }

    /// Abre una transacción si no había ninguna, y dice en qué estado quedó la sesión.
    ///
    /// Es lo que implementa «autocommit apagado»: PostgreSQL no tiene modo autocommit, el `BEGIN` lo
    /// pone el cliente antes de la primera sentencia.
    pub async fn begin_if_needed(&self) -> Result<TxStatus> {
        let status = self.tx_status().await?;
        if status != TxStatus::Idle {
            return Ok(status);
        }

        self.run("BEGIN", Limits::default()).await?;
        Ok(TxStatus::Active)
    }

    /// Confirma la transacción abierta. Sin transacción, el servidor avisa por NOTICE y no falla.
    pub async fn commit(&self) -> Result<()> {
        self.run("COMMIT", Limits::default()).await.map(|_| ())
    }

    /// Revierte la transacción abierta, incluso si está abortada.
    pub async fn rollback(&self) -> Result<()> {
        self.run("ROLLBACK", Limits::default()).await.map(|_| ())
    }
}

/// Nombre del comando, para poder decir «INSERT: 3 filas» en vez de «listo».
///
/// Se deduce del texto y no de la respuesta porque el protocolo simple de tokio-postgres entrega
/// solo el número de filas de `CommandComplete`, sin la etiqueta que lo acompaña.
fn command_tag(sql: &str) -> String {
    /// Solos no dicen nada: `CREATE` puede ser una tabla o un índice.
    const NEEDS_OBJECT: [&str; 3] = ["CREATE", "DROP", "ALTER"];

    /// Hasta dónde estirar la etiqueta en esos casos. `CREATE OR REPLACE FUNCTION` es el más largo
    /// que vale la pena mostrar entero.
    const OBJECTS: [&str; 17] = [
        "TABLE",
        "INDEX",
        "VIEW",
        "FUNCTION",
        "PROCEDURE",
        "SCHEMA",
        "TYPE",
        "DOMAIN",
        "SEQUENCE",
        "TRIGGER",
        "DATABASE",
        "ROLE",
        "USER",
        "EXTENSION",
        "POLICY",
        "PUBLICATION",
        "SUBSCRIPTION",
    ];

    let mut words = skip_comments(sql)
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_uppercase);

    let Some(first) = words.next() else {
        return String::new();
    };
    if !NEEDS_OBJECT.contains(&first.as_str()) {
        return first;
    }

    // Se agrega palabra por palabra hasta la que nombra el objeto, así `CREATE OR REPLACE FUNCTION`
    // y `CREATE MATERIALIZED VIEW` quedan completos en lugar de cortados por la mitad.
    let mut tag = first.clone();
    for word in words.take(3) {
        tag.push(' ');
        tag.push_str(&word);
        if OBJECTS.contains(&word.as_str()) {
            return tag;
        }
    }

    first
}

/// Descarta los comentarios que preceden a la sentencia, para que la etiqueta no salga de ellos.
fn skip_comments(sql: &str) -> &str {
    let mut rest = sql.trim_start();

    loop {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after
                .split_once('\n')
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after
                .split_once("*/")
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else {
            return rest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_etiqueta_sale_de_la_primera_palabra() {
        assert_eq!(command_tag("insert into t values (1)"), "INSERT");
        assert_eq!(command_tag("  UPDATE t SET a = 1"), "UPDATE");
        assert_eq!(command_tag("BEGIN"), "BEGIN");
    }

    #[test]
    fn los_comandos_de_esquema_llevan_el_objeto() {
        assert_eq!(command_tag("create table t (id int)"), "CREATE TABLE");
        assert_eq!(command_tag("DROP   INDEX  i"), "DROP INDEX");
        assert_eq!(
            command_tag("CREATE OR REPLACE FUNCTION f() RETURNS int"),
            "CREATE OR REPLACE FUNCTION",
            "cortarla antes dejaría una etiqueta que no es ningún comando"
        );
        assert_eq!(
            command_tag("CREATE MATERIALIZED VIEW v AS SELECT 1"),
            "CREATE MATERIALIZED VIEW"
        );
    }

    #[test]
    fn sin_objeto_reconocible_alcanza_con_la_primera_palabra() {
        assert_eq!(command_tag("TRUNCATE clientes"), "TRUNCATE");
        assert_eq!(command_tag("ALTER SYSTEM SET work_mem = '64MB'"), "ALTER");
    }

    #[test]
    fn los_comentarios_de_adelante_no_son_la_etiqueta() {
        assert_eq!(
            command_tag("-- crear la tabla\nCREATE TABLE t ()"),
            "CREATE TABLE"
        );
        assert_eq!(command_tag("/* nota */ /* otra */ DELETE FROM t"), "DELETE");
    }

    #[test]
    fn un_texto_sin_comando_no_inventa_etiqueta() {
        assert_eq!(command_tag("-- solo un comentario"), "");
        assert_eq!(command_tag(""), "");
    }
}
