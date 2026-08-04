//! Delegación en `pg_dump` para el DDL de tablas.
//!
//! PostgreSQL expone `pg_get_viewdef`, `pg_get_indexdef`, `pg_get_functiondef` y compañía, pero no
//! existe ningún `pg_get_tabledef`: el DDL de una tabla hay que reconstruirlo columna por columna,
//! con sus valores por omisión, identidades, columnas generadas, restricciones, particionamiento y
//! herencia. Mientras ese generador propio no exista, `pg_dump` da la respuesta correcta y ya
//! resuelta, que es mejor que una aproximación escrita a las apuradas.

use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::backup::tools::{self, Tool};
use crate::conn::profile::{ConnectionProfile, SslMode};
use crate::conn::secret::Password;
use crate::error::{Error, Result};

/// `pg_dump` de una tabla grande con muchos objetos dependientes puede tardar, pero si pasa esto
/// hay algo mal y conviene devolver el control antes que dejar la interfaz colgada.
const TIMEOUT: Duration = Duration::from_secs(30);

fn ssl_mode_env(mode: SslMode) -> &'static str {
    match mode {
        SslMode::Disable => "disable",
        SslMode::Prefer => "prefer",
        SslMode::Require => "require",
        SslMode::VerifyCa => "verify-ca",
        SslMode::VerifyFull => "verify-full",
    }
}

/// Ejecuta `pg_dump --schema-only` restringido a un objeto y devuelve el SQL ya limpio.
pub async fn dump_object(
    profile: &ConnectionProfile,
    password: Option<&Password>,
    database: &str,
    pattern: &str,
) -> Result<String> {
    let binary = tools::require(Tool::PgDump)?;

    let mut command = Command::new(&binary);
    command
        .arg("--schema-only")
        .arg("--no-publications")
        .arg("--no-subscriptions")
        // Sin esto, pg_dump puede quedarse esperando una contraseña por consola que nadie va a
        // escribir, y el proceso queda colgado hasta el timeout.
        .arg("--no-password")
        .arg(format!("--table={pattern}"))
        .arg("--host")
        .arg(&profile.host)
        .arg("--port")
        .arg(profile.port.to_string())
        .arg("--username")
        .arg(&profile.user)
        .arg("--dbname")
        .arg(database)
        .env("PGSSLMODE", ssl_mode_env(profile.ssl_mode))
        .env("PGAPPNAME", "pgforge")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(cert) = &profile.root_cert {
        command.env("PGSSLROOTCERT", cert.as_os_str());
    }
    if let Some(password) = password {
        command.env("PGPASSWORD", password.expose());
    }

    let output = tokio::time::timeout(TIMEOUT, command.output())
        .await
        .map_err(|_| Error::Config(format!("pg_dump no respondió en {}s", TIMEOUT.as_secs())))?
        .map_err(|e| Error::Config(format!("no se pudo ejecutar {}: {e}", binary.display())))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Config(format!(
            "pg_dump falló: {}",
            stderr.trim().lines().next_back().unwrap_or("sin detalle")
        )));
    }

    Ok(clean(&String::from_utf8_lossy(&output.stdout)))
}

/// Saca el preámbulo de `SET`, los comentarios de encabezado y las líneas en blanco repetidas.
///
/// Lo que interesa ver es el DDL del objeto, no la configuración de sesión que `pg_dump` emite
/// para que su salida sea reproducible en otro servidor.
fn clean(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut blank = true;

    for line in raw.lines() {
        let trimmed = line.trim_start();

        let noise = trimmed.starts_with("--")
            || trimmed.starts_with("SET ")
            || trimmed.starts_with("SELECT pg_catalog.set_config");
        if noise {
            continue;
        }

        if trimmed.is_empty() {
            if blank {
                continue;
            }
            blank = true;
        } else {
            blank = false;
        }

        out.push_str(line.trim_end());
        out.push('\n');
    }

    out.trim().to_owned()
}

/// Construye el patrón `--table` de `pg_dump` con los identificadores citados, para que un nombre
/// con mayúsculas o con puntos no se interprete como otra cosa.
pub fn table_pattern(schema: &str, table: &str) -> String {
    format!("{}.{}", quote_pattern(schema), quote_pattern(table))
}

/// `pg_dump` interpreta el argumento como un patrón, así que los comodines de un nombre literal
/// tienen que quedar dentro de comillas dobles.
fn quote_pattern(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limpia_el_preambulo_de_pg_dump() {
        let raw = "--\n-- PostgreSQL database dump\n--\n\nSET statement_timeout = 0;\nSET client_encoding = 'UTF8';\nSELECT pg_catalog.set_config('search_path', '', false);\n\n\nCREATE TABLE public.t (\n    id integer NOT NULL\n);\n\n\n";
        assert_eq!(
            clean(raw),
            "CREATE TABLE public.t (\n    id integer NOT NULL\n);"
        );
    }

    #[test]
    fn cita_los_identificadores_del_patron() {
        assert_eq!(
            table_pattern("public", "mi tabla"),
            "\"public\".\"mi tabla\""
        );
        assert_eq!(
            table_pattern("public", "co\"millas"),
            "\"public\".\"co\"\"millas\""
        );
        // Un nombre con comodines tiene que quedar literal, no como patrón.
        assert_eq!(table_pattern("public", "a_b%"), "\"public\".\"a_b%\"");
    }
}
