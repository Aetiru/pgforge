//! Mover datos en bloque con `COPY`: exportar una tabla o una consulta a un archivo, e importar un
//! archivo a una tabla.
//!
//! Es el complemento de [`crate::data::page`] y [`crate::data::edit`], que leen y cambian de a una
//! fila. Acá el volumen es el problema: una tabla de millones de filas no puede pasar por memoria,
//! así que el archivo se escribe y se lee por trozos a medida que el `COPY` avanza, nunca entero.
//!
//! Como toda mutación del proyecto, se parte en dos: [`export_command`] e [`import_command`] arman
//! el texto del `COPY` y son **puros** —lo único verificable sin servidor, y la garantía de que la
//! interfaz muestra exactamente lo que se va a ejecutar—, y [`export_to_file`]/[`import_from_file`]
//! lo ejecutan.

use std::path::Path;

use bytes::Bytes;
use futures_util::{SinkExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_postgres::CopyInSink;

use crate::conn::ServerHandle;
use crate::ddl::{qualified, quote_ident};
use crate::error::{Error, Result};

/// Cada cuántos bytes se avisa el avance. Reportar cada trozo inundaría el canal sin que la barra se
/// mueva de forma perceptible; un cuarto de mega es un paso visible sin ser ruidoso.
const REPORT_EVERY: u64 = 256 * 1024;

/// Tamaño del trozo con que se lee el archivo al importar. 64 KiB es el punto habitual entre pocas
/// llamadas al sistema y no reservar de más.
const CHUNK: usize = 64 * 1024;

/// Formato del archivo, tal como lo entiende `COPY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CopyFormat {
    Csv,
    Text,
    Binary,
}

impl CopyFormat {
    fn keyword(self) -> &'static str {
        match self {
            CopyFormat::Csv => "csv",
            CopyFormat::Text => "text",
            CopyFormat::Binary => "binary",
        }
    }
}

/// Opciones de formato de texto y CSV. En binario no se usa ninguna: el formato binario de `COPY`
/// no es portable entre versiones de PostgreSQL y no admite estos ajustes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextOptions {
    /// Primera línea con los nombres de columna. Solo CSV.
    pub header: bool,
    /// Separador de campos. Un solo carácter.
    pub delimiter: Option<String>,
    /// Carácter de comillas. Solo CSV.
    pub quote: Option<String>,
    /// Texto que representa un `NULL` (distinto de la cadena vacía).
    pub null: Option<String>,
}

/// De dónde salen las filas a exportar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExportSource {
    /// Una tabla entera, o algunas de sus columnas.
    #[serde(rename_all = "camelCase")]
    Table {
        schema: String,
        table: String,
        #[serde(default)]
        columns: Vec<String>,
    },
    /// El resultado de una consulta escrita por el usuario.
    #[serde(rename_all = "camelCase")]
    Query { sql: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSpec {
    pub source: ExportSource,
    pub format: CopyFormat,
    #[serde(default)]
    pub options: TextOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSpec {
    pub schema: String,
    pub table: String,
    /// Si va vacío, `COPY` usa todas las columnas de la tabla en su orden.
    #[serde(default)]
    pub columns: Vec<String>,
    pub format: CopyFormat,
    #[serde(default)]
    pub options: TextOptions,
}

/// El texto del `COPY` que se ejecutaría. Lo que alimenta la vista previa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyCommand {
    pub sql: String,
}

/// Cómo terminó una exportación o importación.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub bytes: u64,
    /// Filas procesadas. En importación las informa el servidor al confirmar el `COPY`; en
    /// exportación `COPY TO` no las devuelve, así que va `None` y la interfaz muestra solo el tamaño.
    pub rows: Option<u64>,
}

/// El `COPY ... TO STDOUT` para `spec`. Puro.
pub fn export_command(spec: &ExportSpec) -> Result<CopyCommand> {
    let source = match &spec.source {
        ExportSource::Table {
            schema,
            table,
            columns,
        } => {
            if table.trim().is_empty() {
                return Err(Error::Config("falta la tabla a exportar".to_owned()));
            }
            let mut source = qualified(schema, table);
            if !columns.is_empty() {
                source.push_str(&format!(" ({})", column_list(columns)));
            }
            source
        }
        ExportSource::Query { sql } => {
            // El `;` final es cómodo de dejar al copiar del editor, pero `COPY (...)` no lo admite.
            let trimmed = sql.trim().trim_end_matches(';').trim();
            if trimmed.is_empty() {
                return Err(Error::Config(
                    "la consulta a exportar está vacía".to_owned(),
                ));
            }
            // Va cruda a propósito: es la misma frontera de confianza que el editor de consultas, la
            // ejecuta el propio usuario con sus privilegios y la valida el servidor al correr.
            format!("({trimmed})")
        }
    };

    let sql = format!(
        "COPY {source} TO STDOUT WITH ({})",
        with_clause(spec.format, &spec.options)?
    );
    Ok(CopyCommand { sql })
}

/// El `COPY ... FROM STDIN` para `spec`. Puro.
pub fn import_command(spec: &ImportSpec) -> Result<CopyCommand> {
    if spec.table.trim().is_empty() {
        return Err(Error::Config("falta la tabla de destino".to_owned()));
    }

    let mut target = qualified(&spec.schema, &spec.table);
    if !spec.columns.is_empty() {
        target.push_str(&format!(" ({})", column_list(&spec.columns)));
    }

    let sql = format!(
        "COPY {target} FROM STDIN WITH ({})",
        with_clause(spec.format, &spec.options)?
    );
    Ok(CopyCommand { sql })
}

fn column_list(columns: &[String]) -> String {
    columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Arma el paréntesis de opciones de `COPY`. Rechaza acá las combinaciones que el servidor
/// rechazaría al ejecutar —binario con opciones de texto, comillas o encabezado fuera de CSV—
/// porque un error de vista previa se corrige antes de tocar el disco, y uno de ejecución no.
fn with_clause(format: CopyFormat, options: &TextOptions) -> Result<String> {
    let mut parts = vec![format!("FORMAT {}", format.keyword())];

    match format {
        CopyFormat::Binary => {
            if options.header
                || options.delimiter.is_some()
                || options.quote.is_some()
                || options.null.is_some()
            {
                return Err(Error::Config(
                    "el formato binario no admite delimitador, comillas, encabezado ni texto de NULL"
                        .to_owned(),
                ));
            }
        }
        CopyFormat::Text | CopyFormat::Csv => {
            if options.header {
                if format != CopyFormat::Csv {
                    return Err(Error::Config(
                        "el encabezado (HEADER) solo vale para el formato CSV".to_owned(),
                    ));
                }
                parts.push("HEADER".to_owned());
            }
            if let Some(delimiter) = &options.delimiter {
                parts.push(format!(
                    "DELIMITER {}",
                    single_char("el delimitador", delimiter)?
                ));
            }
            if let Some(quote) = &options.quote {
                if format != CopyFormat::Csv {
                    return Err(Error::Config(
                        "las comillas (QUOTE) solo valen para el formato CSV".to_owned(),
                    ));
                }
                parts.push(format!("QUOTE {}", single_char("las comillas", quote)?));
            }
            if let Some(null) = &options.null {
                parts.push(format!("NULL {}", literal(null)));
            }
        }
    }

    Ok(parts.join(", "))
}

/// Un literal de cadena SQL: comillas simples, duplicando las de adentro. `COPY` toma delimitador,
/// comillas y texto de NULL como literales, no como identificadores.
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Igual que [`literal`], pero exige que sea un solo carácter: `DELIMITER` y `QUOTE` no aceptan más,
/// y avisarlo acá evita un error del servidor a mitad de una exportación.
fn single_char(name: &str, value: &str) -> Result<String> {
    if value.chars().count() != 1 {
        return Err(Error::Config(format!(
            "{name} tiene que ser un solo carácter"
        )));
    }
    Ok(literal(value))
}

/// Exporta a `path`, transmitiendo los bytes escritos por `progress` y abortando si llega algo por
/// `cancel`.
///
/// Usa una sesión dedicada y no una conexión del pool: exportar una tabla enorme no debe dejar al
/// explorador sin conexiones, y cancelar necesita un token propio (la conexión que ejecuta el `COPY`
/// está ocupada). Sin `statement_timeout`, como el mantenimiento: una exportación tarda lo que tarda
/// y matarla a los treinta segundos sería peor que no tenerla.
pub async fn export_to_file(
    handle: &ServerHandle,
    database: &str,
    spec: &ExportSpec,
    path: &Path,
    progress: mpsc::Sender<u64>,
    cancel: oneshot::Receiver<()>,
) -> Result<Outcome> {
    let command = export_command(spec)?;

    let session = handle.open_session(database, None).await?;
    let token = session.cancel_token();

    let mut file = File::create(path)
        .await
        .map_err(|e| Error::Config(format!("no se pudo crear {}: {e}", path.display())))?;

    let stream = session.client().copy_out(&command.sql).await?;
    tokio::pin!(stream);

    let mut cancel = cancel;
    let mut bytes: u64 = 0;
    let mut since_report: u64 = 0;

    let result = loop {
        tokio::select! {
            // La cancelación gana sobre un trozo listo: si el usuario apretó cancelar, no tiene
            // sentido escribir una fila más antes de atenderlo.
            biased;
            _ = &mut cancel => break Err(Error::Canceled),
            chunk = stream.try_next() => match chunk {
                Ok(Some(data)) => {
                    if let Err(e) = file.write_all(&data).await {
                        break Err(Error::Config(format!(
                            "no se pudo escribir en {}: {e}", path.display()
                        )));
                    }
                    bytes += data.len() as u64;
                    since_report += data.len() as u64;
                    if since_report >= REPORT_EVERY {
                        since_report = 0;
                        let _ = progress.send(bytes).await;
                    }
                }
                Ok(None) => break Ok(()),
                Err(e) => break Err(Error::from(e)),
            }
        }
    };

    match result {
        Ok(()) => {
            file.flush().await.ok();
            let _ = progress.send(bytes).await;
            Ok(Outcome { bytes, rows: None })
        }
        Err(err) => {
            // El `COPY TO` sigue vivo en el servidor hasta que se le avisa, y el archivo a medio
            // escribir no sirve —igual que un backup truncado—, así que se cancela y se borra.
            if matches!(err, Error::Canceled) {
                let _ = handle.cancel(&token).await;
            }
            let _ = tokio::fs::remove_file(path).await;
            Err(err)
        }
    }
}

/// Importa `path`, transmitiendo los bytes leídos por `progress` y abortando si llega algo por
/// `cancel`.
///
/// Un `COPY FROM` es una sola sentencia: o entra el archivo entero o no entra nada, sin necesidad de
/// una transacción explícita. Si algo falla —una fila mal formada, una cancelación— el sink se
/// suelta sin confirmar y el servidor descarta todo lo cargado.
pub async fn import_from_file(
    handle: &ServerHandle,
    database: &str,
    spec: &ImportSpec,
    path: &Path,
    progress: mpsc::Sender<u64>,
    cancel: oneshot::Receiver<()>,
) -> Result<Outcome> {
    let command = import_command(spec)?;

    let session = handle.open_session(database, None).await?;

    let mut file = File::open(path)
        .await
        .map_err(|e| Error::Config(format!("no se pudo abrir {}: {e}", path.display())))?;

    let sink: CopyInSink<Bytes> = session.client().copy_in(&command.sql).await?;
    tokio::pin!(sink);

    let mut cancel = cancel;
    let mut buffer = vec![0u8; CHUNK];
    let mut bytes: u64 = 0;
    let mut since_report: u64 = 0;

    let result = loop {
        tokio::select! {
            biased;
            _ = &mut cancel => break Err(Error::Canceled),
            read = file.read(&mut buffer) => match read {
                Ok(0) => break Ok(()),
                Ok(n) => {
                    if let Err(e) = sink.send(Bytes::copy_from_slice(&buffer[..n])).await {
                        break Err(Error::from(e));
                    }
                    bytes += n as u64;
                    since_report += n as u64;
                    if since_report >= REPORT_EVERY {
                        since_report = 0;
                        let _ = progress.send(bytes).await;
                    }
                }
                Err(e) => break Err(Error::Config(format!(
                    "no se pudo leer {}: {e}", path.display()
                ))),
            }
        }
    };

    match result {
        // `finish()` confirma el `COPY` y devuelve las filas insertadas; soltar el sink sin llamarlo
        // lo aborta.
        Ok(()) => {
            let rows = sink.finish().await?;
            let _ = progress.send(bytes).await;
            Ok(Outcome {
                bytes,
                rows: Some(rows),
            })
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(columns: &[&str]) -> ExportSource {
        ExportSource::Table {
            schema: "public".into(),
            table: "clientes".into(),
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    fn export(source: ExportSource, format: CopyFormat, options: TextOptions) -> String {
        export_command(&ExportSpec {
            source,
            format,
            options,
        })
        .unwrap()
        .sql
    }

    #[test]
    fn exporta_una_tabla_entera_en_csv_con_encabezado() {
        let sql = export(
            table(&[]),
            CopyFormat::Csv,
            TextOptions {
                header: true,
                ..Default::default()
            },
        );
        assert_eq!(
            sql,
            "COPY public.clientes TO STDOUT WITH (FORMAT csv, HEADER)"
        );
    }

    #[test]
    fn exporta_columnas_elegidas_y_las_cita_si_hace_falta() {
        let sql = export(
            table(&["nombre", "correo electrónico"]),
            CopyFormat::Csv,
            TextOptions::default(),
        );
        assert!(
            sql.contains("public.clientes (nombre, \"correo electrónico\")"),
            "{sql}"
        );
    }

    #[test]
    fn exporta_el_resultado_de_una_consulta_cruda() {
        let sql = export(
            ExportSource::Query {
                sql: "SELECT id, nombre FROM clientes WHERE activo;".into(),
            },
            CopyFormat::Csv,
            TextOptions::default(),
        );
        assert_eq!(
            sql,
            "COPY (SELECT id, nombre FROM clientes WHERE activo) TO STDOUT WITH (FORMAT csv)"
        );
    }

    #[test]
    fn delimitador_comillas_y_null_a_medida() {
        let sql = export(
            table(&[]),
            CopyFormat::Csv,
            TextOptions {
                header: false,
                delimiter: Some(";".into()),
                quote: Some("'".into()),
                null: Some("\\N".into()),
            },
        );
        assert_eq!(
            sql,
            "COPY public.clientes TO STDOUT WITH (FORMAT csv, DELIMITER ';', QUOTE '''', NULL '\\N')"
        );
    }

    #[test]
    fn el_texto_de_null_escapa_las_comillas_simples() {
        let sql = export(
            table(&[]),
            CopyFormat::Text,
            TextOptions {
                null: Some("d'oh".into()),
                ..Default::default()
            },
        );
        assert!(sql.contains("NULL 'd''oh'"), "{sql}");
    }

    #[test]
    fn el_binario_no_admite_opciones_de_texto() {
        assert!(export_command(&ExportSpec {
            source: table(&[]),
            format: CopyFormat::Binary,
            options: TextOptions {
                delimiter: Some(",".into()),
                ..Default::default()
            },
        })
        .is_err());
    }

    #[test]
    fn el_encabezado_y_las_comillas_solo_valen_para_csv() {
        assert!(export_command(&ExportSpec {
            source: table(&[]),
            format: CopyFormat::Text,
            options: TextOptions {
                header: true,
                ..Default::default()
            },
        })
        .is_err());
        assert!(export_command(&ExportSpec {
            source: table(&[]),
            format: CopyFormat::Text,
            options: TextOptions {
                quote: Some("\"".into()),
                ..Default::default()
            },
        })
        .is_err());
    }

    #[test]
    fn un_delimitador_de_mas_de_un_caracter_no_se_acepta() {
        assert!(export_command(&ExportSpec {
            source: table(&[]),
            format: CopyFormat::Csv,
            options: TextOptions {
                delimiter: Some("||".into()),
                ..Default::default()
            },
        })
        .is_err());
    }

    #[test]
    fn una_consulta_vacia_no_se_exporta() {
        assert!(export_command(&ExportSpec {
            source: ExportSource::Query {
                sql: "  ;  ".into()
            },
            format: CopyFormat::Csv,
            options: TextOptions::default(),
        })
        .is_err());
    }

    #[test]
    fn importa_a_una_tabla_con_columnas_elegidas() {
        let sql = import_command(&ImportSpec {
            schema: "public".into(),
            table: "clientes".into(),
            columns: vec!["id".into(), "nombre".into()],
            format: CopyFormat::Csv,
            options: TextOptions {
                header: true,
                ..Default::default()
            },
        })
        .unwrap()
        .sql;
        assert_eq!(
            sql,
            "COPY public.clientes (id, nombre) FROM STDIN WITH (FORMAT csv, HEADER)"
        );
    }

    #[test]
    fn importar_sin_tabla_no_se_genera() {
        assert!(import_command(&ImportSpec {
            schema: "public".into(),
            table: "  ".into(),
            columns: vec![],
            format: CopyFormat::Csv,
            options: TextOptions::default(),
        })
        .is_err());
    }
}
