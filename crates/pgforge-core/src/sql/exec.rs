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
use crate::sql::lex::{lex, TokenKind};

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

/// Nombre y tipo de una columna del resultado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnType {
    pub name: String,
    /// El nombre del tipo tal como lo llama PostgreSQL: `int8`, `timestamptz`, `jsonb`.
    pub type_name: String,
}

/// De qué tabla sale un resultado y qué columna de ella es cada columna del resultado.
///
/// `columns[i]` es el nombre en la tabla de la i-ésima columna del resultado, o `None` si no es una
/// columna de esa tabla (una expresión, un alias de función, algo que viene de otra tabla).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSource {
    pub oid: u32,
    pub columns: Vec<Option<String>>,
}

/// La única tabla de la que salen las columnas de un resultado, o por qué no se puede editar.
///
/// Es la decisión que habilita escribir sobre un resultado, así que es pura y se prueba sola: ante
/// la duda —ninguna columna viene de una tabla, o vienen de varias— no se ofrece editar.
fn single_origin(origins: &[Option<(u32, i16)>]) -> std::result::Result<u32, &'static str> {
    let mut tables = origins.iter().flatten().map(|(oid, _)| *oid);
    let Some(first) = tables.next() else {
        return Err("El resultado no trae columnas de una tabla: son expresiones o funciones.");
    };
    if tables.any(|oid| oid != first) {
        return Err("El resultado sale de más de una tabla; solo se edita el de una sola.");
    }
    Ok(first)
}

/// El veredicto de revisar una sentencia sin ejecutarla.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Check {
    Ok,
    Error {
        message: String,
        /// Dónde empieza el problema dentro del texto recibido: en caracteres y con base 0 (el
        /// servidor lo cuenta con base 1).
        position: Option<usize>,
        hint: Option<String>,
    },
    /// No se revisó, y no es un fallo: la interfaz no muestra nada.
    Skipped {
        reason: String,
    },
}

/// La posición del servidor (base 1, en caracteres) a la del editor (base 0).
fn zero_based(position: Option<u32>) -> Option<usize> {
    position.map(|p| usize::try_from(p).unwrap_or(1).saturating_sub(1))
}

impl Check {
    fn skipped(reason: &str) -> Self {
        Check::Skipped {
            reason: reason.to_owned(),
        }
    }
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
    /// El pid del backend de esta sesión, leído al abrir: es con lo que otra conexión pregunta qué
    /// está esperando la consulta mientras esta sigue ocupada ejecutándola.
    pid: i32,
}

impl QuerySession {
    pub async fn open(handle: &ServerHandle, database: &str) -> Result<Self> {
        // Con el `statement_timeout` del perfil, al revés que el mantenimiento: una consulta
        // escrita a mano es justamente lo que ese límite existe para acotar.
        let session = handle
            .open_session(database, handle.profile.statement_timeout_ms)
            .await?;

        let pid: i32 = session
            .client()
            .query_one("SELECT pg_backend_pid()", &[])
            .await?
            .get(0);

        Ok(Self { session, pid })
    }

    /// El pid del backend de esta sesión.
    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// Cuántas filas tiene una tabla según las estadísticas del servidor, sin contarlas.
    ///
    /// Es `pg_class.reltuples`: una estimación, que es justo lo que hace falta para avisar «esto es
    /// enorme». `None` si la tabla no existe o nunca se analizó (PostgreSQL 14 en adelante guarda
    /// `-1` ahí; antes guardaba `0`, que se trata igual: no se sabe). El nombre se resuelve con
    /// `to_regclass`, o sea con el `search_path` de la propia sesión, que es contra lo que se escribió.
    pub async fn estimated_rows(&self, schema: Option<&str>, table: &str) -> Result<Option<i64>> {
        let name = match schema {
            Some(schema) => format!(
                "{}.{}",
                crate::ddl::quote_ident(schema),
                crate::ddl::quote_ident(table)
            ),
            None => crate::ddl::quote_ident(table),
        };
        let row = self
            .session
            .client()
            .query_opt(
                "SELECT c.reltuples::int8
                   FROM pg_catalog.pg_class c
                  WHERE c.oid = pg_catalog.to_regclass($1)",
                &[&name],
            )
            .await?;
        // Sin fila, la tabla no existe (o no se ve): no es un error de quien pregunta.
        Ok(row.map(|row| row.get::<_, i64>(0)).filter(|rows| *rows > 0))
    }

    /// Pone (o saca) el tiempo máximo de cada sentencia de esta sesión.
    ///
    /// `Some(0)` es «sin límite» —lo que `statement_timeout = 0` significa—; `None` vuelve al valor
    /// con que se abrió la sesión, el del perfil. El número viene de la interfaz y se interpola:
    /// por eso es un `u64`, que no puede llevar más que dígitos.
    pub async fn set_statement_timeout(&self, millis: Option<u64>) -> Result<()> {
        let sql = match millis {
            Some(ms) => format!("SET statement_timeout = {ms}"),
            None => "RESET statement_timeout".to_owned(),
        };
        self.session.client().batch_execute(&sql).await?;
        Ok(())
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

    /// Como [`Self::run`], pero pidiéndole al **servidor** que corte en el techo.
    ///
    /// `run` solo descarta lo que sobra de este lado: el servidor igual produce y manda la tabla
    /// entera, así que un `SELECT *` sobre millones de filas tarda lo mismo con un techo de 200 que
    /// sin él. Acá la lectura simple se envuelve en un `LIMIT` de una fila más que el techo —esa
    /// de más es lo que dice que hubo recorte—. Una escritura (`UPDATE … RETURNING`) nunca se
    /// envuelve: cortarla a medias no es una opción.
    ///
    /// Si la envuelta falla, se vuelve a correr la original: la posición del error que devuelve
    /// PostgreSQL se cuenta sobre el texto que recibió, y el editor la marca sobre el que escribió
    /// el usuario.
    pub async fn run_capped(&self, sql: &str, limits: Limits) -> Result<Outcome> {
        if limits.max_rows == usize::MAX || !is_plain_read(sql) {
            return self.run(sql, limits).await;
        }

        match self.run(&capped(sql, limits.max_rows), limits).await {
            Err(Error::Database { .. }) => self.run(sql, limits).await,
            other => other,
        }
    }

    /// Los tipos de las columnas que devolvería esta sentencia.
    ///
    /// El protocolo simple con el que se ejecuta no los trae: su `RowDescription` expone el nombre
    /// de cada columna y nada más. La única forma de saberlos es **preparar** la sentencia —el
    /// servidor la analiza y la planifica, pero no la ejecuta— y leer la descripción del protocolo
    /// extendido.
    ///
    /// Por eso va aparte y a pedido, en vez de sumarse a [`Self::run`]: cuesta otra vuelta al
    /// servidor y una planificación de más, y quien no mira los tipos no tiene por qué pagarlas.
    /// Vale para una sola sentencia: preparar un script entero lo rechaza el servidor.
    pub async fn column_types(&self, sql: &str) -> Result<Vec<ColumnType>> {
        // El `;` final es cómodo de dejar al escribir, pero acá se prepara una sola sentencia.
        let statement = self
            .session
            .client()
            .prepare(sql.trim().trim_end_matches(';'))
            .await?;

        Ok(statement
            .columns()
            .iter()
            .map(|column| ColumnType {
                name: column.name().to_owned(),
                type_name: column.type_().name().to_owned(),
            })
            .collect())
    }

    /// Ejecuta un conteo previo —`SELECT count(*) FROM (SELECT 1 FROM … LIMIT n) AS _n`— y devuelve
    /// el número. Es lo que la interfaz arma para decir cuántas filas tocaría un `UPDATE` o un
    /// `DELETE` antes de correrlo.
    ///
    /// Solo acepta esa forma: esto no es un segundo camino para ejecutar SQL, y lo que no empieza así
    /// se rechaza. Va por la sesión de la pestaña, que es la que ve lo que su transacción ya cambió.
    pub async fn count(&self, sql: &str) -> Result<i64> {
        if !sql.starts_with("SELECT count(*) FROM (SELECT 1 FROM ") {
            return Err(Error::Config("no es un conteo previo".into()));
        }
        let row = self.session.client().query_one(sql, &[]).await?;
        Ok(row.get(0))
    }

    /// Revisa una sentencia con el servidor **sin ejecutarla**: `prepare` manda solo el `Parse`, que
    /// valida la sintaxis y resuelve los nombres (tablas, columnas, tipos) pero no corre nada.
    ///
    /// Dos casos se saltean en vez de arriesgarse:
    /// - **Con una transacción abierta o fallida.** Resolver nombres toma un `ACCESS SHARE` sobre
    ///   cada tabla, y adentro de una transacción ese bloqueo se queda hasta el `COMMIT`: un
    ///   `ALTER TABLE` de otra persona esperaría por una revisión que se hizo mientras alguien
    ///   escribía. Con la transacción fallida, además, el servidor rechaza todo con `25P02`.
    /// - **Con más de una sentencia.** El protocolo extendido no acepta varias en un `Parse`, y el
    ///   error resultante no es un problema del texto.
    ///
    /// Si otra sesión tiene la tabla bloqueada la revisión **espera**: se le pone un
    /// `lock_timeout` corto y se restaura el que tenía la pestaña —incluso si falla—, que es lo único
    /// que impide que una pausa al escribir se quede colgada detrás de un `ALTER TABLE` ajeno.
    /// Un error de conexión no es un error del SQL y se propaga como siempre.
    pub async fn check(&self, sql: &str) -> Result<Check> {
        // Solo el final se recorta: lo de adelante no mueve ninguna posición de error.
        let text = sql.trim_end().trim_end_matches(';');
        if text.trim().is_empty() {
            return Ok(Check::skipped("vacía"));
        }
        if crate::sql::split::split(text).len() > 1 {
            return Ok(Check::skipped("varias sentencias"));
        }
        if self.tx_status().await? != TxStatus::Idle {
            return Ok(Check::skipped("transacción abierta"));
        }

        let client = self.session.client();
        let previous: String = client
            .query_one("SELECT current_setting('lock_timeout')", &[])
            .await?
            .get(0);
        client
            .batch_execute("SET lock_timeout = '300ms'")
            .await
            .map_err(Error::from)?;

        let outcome = client.prepare(text).await;

        // Restaurar es parte de la revisión, no un detalle: sin esto, `SHOW lock_timeout` de la
        // pestaña mostraría 300 ms para siempre. El valor se cita como literal.
        let restore = format!("SET lock_timeout = '{}'", previous.replace('\'', "''"));
        client.batch_execute(&restore).await.map_err(Error::from)?;

        match outcome {
            Ok(_) => Ok(Check::Ok),
            Err(error) => match Error::from(error) {
                Error::Database {
                    code,
                    message,
                    hint,
                    position,
                    ..
                } => {
                    // Un `lock_timeout` (55P03) y una cancelación (57014) no dicen nada del texto.
                    if code == "55P03" || code == "57014" {
                        return Ok(Check::skipped("tabla ocupada"));
                    }
                    Ok(Check::Error {
                        message,
                        position: zero_based(position),
                        hint,
                    })
                }
                other => Err(other),
            },
        }
    }

    /// De qué tabla sale el resultado de la sentencia, sin ejecutarla.
    ///
    /// Se le pregunta al servidor —`prepare` devuelve, por cada columna, el oid de la tabla de
    /// origen y su número de columna— y no se deduce leyendo el `FROM`: un alias, una vista o un
    /// `SELECT *` hacen que el texto no diga qué columna es cuál. Una expresión no tiene origen y
    /// por eso queda en `None`. Para una vista el origen es la vista misma, y la grilla de edición
    /// la rechaza por solo lectura.
    pub async fn edit_source(&self, sql: &str) -> Result<EditSource> {
        let client = self.session.client();
        let statement = client.prepare(sql.trim().trim_end_matches(';')).await?;

        let origins: Vec<Option<(u32, i16)>> = statement
            .columns()
            .iter()
            .map(|column| match (column.table_oid(), column.column_id()) {
                (Some(oid), Some(id)) if oid != 0 => Some((oid, id)),
                _ => None,
            })
            .collect();

        let oid = single_origin(&origins).map_err(|reason| Error::Config(reason.to_owned()))?;

        let names: std::collections::HashMap<i16, String> = client
            .query(
                "SELECT attnum, attname::text
                   FROM pg_catalog.pg_attribute
                  WHERE attrelid = $1 AND attnum > 0 AND NOT attisdropped",
                &[&oid],
            )
            .await?
            .into_iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();

        let columns = origins
            .iter()
            .map(|origin| match origin {
                Some((table, id)) if *table == oid => names.get(id).cloned(),
                _ => None,
            })
            .collect();

        Ok(EditSource { oid, columns })
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

/// `true` si la sentencia solo lee y se puede envolver en `SELECT * FROM (…) LIMIT n`.
///
/// Conservadora a propósito: ante la duda dice que no y la sentencia corre tal cual. Entran
/// `SELECT`, `WITH`, `TABLE` y `VALUES`; una palabra de escritura o de `SELECT … INTO` en cualquier
/// parte la saca (un `WITH` puede traer un `DELETE … RETURNING` adentro).
pub(crate) fn is_plain_read(sql: &str) -> bool {
    let words: Vec<String> = lex(sql)
        .into_iter()
        .filter(|token| token.kind == TokenKind::Word)
        .map(|token| token.text.to_ascii_lowercase())
        .collect();

    let Some(first) = words.first() else {
        return false;
    };
    if !matches!(first.as_str(), "select" | "with" | "table" | "values") {
        return false;
    }

    !words.iter().any(|word| {
        matches!(
            word.as_str(),
            "into" | "insert" | "update" | "delete" | "merge" | "truncate"
        )
    })
}

/// La lectura envuelta en el techo, con una fila de más para saber si hubo recorte.
///
/// El salto de línea antes del paréntesis es lo que evita que un comentario `--` al final del
/// texto se coma el cierre.
pub(crate) fn capped(sql: &str, max_rows: usize) -> String {
    format!(
        "SELECT * FROM (\n{sql}\n) AS pgforge_cap LIMIT {}",
        max_rows.saturating_add(1)
    )
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
    #[test]
    fn la_posicion_del_servidor_pasa_a_base_cero() {
        assert_eq!(super::zero_based(Some(1)), Some(0));
        assert_eq!(super::zero_based(Some(15)), Some(14));
        // El servidor nunca manda 0, pero un valor así no puede dar un desbordamiento.
        assert_eq!(super::zero_based(Some(0)), Some(0));
        assert_eq!(super::zero_based(None), None);
    }

    #[test]
    fn el_veredicto_de_la_revision_viaja_etiquetado() {
        let json = serde_json::to_value(super::Check::Error {
            message: "no existe".into(),
            position: Some(7),
            hint: None,
        })
        .unwrap();
        assert_eq!(json["status"], "error");
        assert_eq!(json["position"], 7);
        let json = serde_json::to_value(super::Check::skipped("vacía")).unwrap();
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["reason"], "vacía");
        assert_eq!(
            serde_json::to_value(super::Check::Ok).unwrap()["status"],
            "ok"
        );
    }

    use super::{capped, is_plain_read, single_origin};

    #[test]
    fn el_origen_de_un_resultado_es_una_sola_tabla() {
        // Una expresión no tiene origen y no impide editar el resto.
        assert_eq!(single_origin(&[Some((10, 1)), None, Some((10, 3))]), Ok(10));
        assert!(single_origin(&[None, None]).is_err());
        assert!(single_origin(&[]).is_err());
        assert!(single_origin(&[Some((10, 1)), Some((11, 1))]).is_err());
    }

    #[test]
    fn reconoce_las_lecturas_simples_y_deja_pasar_las_escrituras() {
        assert!(is_plain_read("SELECT * FROM progvehiculos"));
        assert!(is_plain_read("  -- nota\n select 1"));
        assert!(is_plain_read("WITH a AS (SELECT 1) SELECT * FROM a"));
        assert!(is_plain_read("TABLE clientes"));
        assert!(!is_plain_read("UPDATE t SET a = 1 RETURNING *"));
        assert!(!is_plain_read(
            "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d"
        ));
        assert!(!is_plain_read("SELECT * INTO copia FROM t"));
        assert!(!is_plain_read("SELECT * FROM t FOR UPDATE"));
        assert!(!is_plain_read("EXPLAIN SELECT 1"));
        assert!(!is_plain_read(""));
    }

    #[test]
    fn el_techo_pide_una_fila_de_mas_y_protege_el_comentario_final() {
        let sql = capped("SELECT 1 -- hola", 200);
        assert!(sql.ends_with("LIMIT 201"));
        assert!(sql.contains("-- hola\n)"));
    }

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
