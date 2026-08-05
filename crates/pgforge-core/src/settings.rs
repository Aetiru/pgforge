//! Configuración del servidor: leer y cambiar los parámetros de `pg_settings`.
//!
//! Los parámetros son del clúster, no de una base, así que se leen desde la base por defecto. Cambiar
//! uno es `ALTER SYSTEM SET`, que escribe `postgresql.auto.conf` y toma efecto al recargar
//! (`pg_reload_conf`) para los de contexto `sighup`/`user`, o al reiniciar para los `postmaster`.
//!
//! El valor va como literal de cadena y el servidor lo reinterpreta según el tipo del parámetro
//! (`work_mem = '8MB'`, `log_connections = 'on'`); es la misma frontera de confianza que el resto del
//! DDL: lo ejecuta el propio usuario con sus privilegios, y `ALTER SYSTEM` pide superusuario.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::ddl::table::Statement;
use crate::error::Result;

/// Un parámetro de configuración tal como lo reporta `pg_settings`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    pub name: String,
    pub value: String,
    pub unit: Option<String>,
    pub category: String,
    pub short_desc: String,
    /// Cuándo se puede cambiar: `internal` (nunca), `postmaster` (con reinicio), `sighup` (con
    /// recarga), `superuser`/`user`/`backend`/`superuser-backend` (en caliente).
    pub context: String,
    /// `bool`, `integer`, `real`, `enum` o `string`: decide el widget de edición.
    pub var_type: String,
    pub min_val: Option<String>,
    pub max_val: Option<String>,
    /// Valores admitidos cuando `var_type` es `enum`; vacío en el resto.
    pub enum_vals: Vec<String>,
    /// El valor de fábrica, para poder ofrecer «restablecer». `None` en los pocos que no tienen.
    pub boot_val: Option<String>,
    pub reset_val: Option<String>,
    /// De dónde sale el valor actual: `default`, `configuration file`, `override`, `session`, …
    pub source: String,
    /// El valor ya se cambió pero necesita un reinicio para tomar efecto.
    pub pending_restart: bool,
}

const SETTINGS_SQL: &str = "
    SELECT name, setting, unit, category, short_desc, context, vartype,
           min_val, max_val, enumvals, boot_val, reset_val, source, pending_restart
      FROM pg_catalog.pg_settings
     ORDER BY category, name
";

pub async fn list(handle: &ServerHandle) -> Result<Vec<Setting>> {
    let client = handle.client(handle.default_database()).await?;
    let rows = client.query(SETTINGS_SQL, &[]).await?;

    Ok(rows
        .into_iter()
        .map(|row| Setting {
            name: row.get(0),
            value: row.get(1),
            unit: row.get(2),
            category: row.get(3),
            short_desc: row.get(4),
            context: row.get(5),
            var_type: row.get(6),
            min_val: row.get(7),
            max_val: row.get(8),
            enum_vals: row.get::<_, Option<Vec<String>>>(9).unwrap_or_default(),
            boot_val: row.get(10),
            reset_val: row.get(11),
            source: row.get(12),
            pending_restart: row.get(13),
        })
        .collect())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SettingChange {
    Set { name: String, value: String },
    Reset { name: String },
}

impl SettingChange {
    fn name(&self) -> &str {
        match self {
            SettingChange::Set { name, .. } | SettingChange::Reset { name } => name,
        }
    }
}

/// Comillas simples dobladas: el mismo criterio que `ddl::role`.
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Arma las sentencias `ALTER SYSTEM`. Es una función pura para poder verificarla sin servidor.
///
/// El nombre del parámetro se emite tal cual: sale de `pg_settings` (es confiable) y citarlo como
/// identificador rompería los nombres con punto de las extensiones (`auto_explain.log_min_duration`).
pub fn statements(changes: &[SettingChange]) -> Vec<Statement> {
    changes
        .iter()
        .map(|change| {
            let sql = match change {
                SettingChange::Set { name, value } => {
                    format!("ALTER SYSTEM SET {name} = {}", quote_literal(value))
                }
                SettingChange::Reset { name } => format!("ALTER SYSTEM RESET {name}"),
            };
            Statement { sql }
        })
        .collect()
}

/// Aplica los cambios y recarga la configuración. Devuelve `true` si alguno de los que se tocaron
/// necesita un reinicio para tomar efecto (los de contexto `postmaster`).
pub async fn apply(handle: &ServerHandle, changes: &[SettingChange]) -> Result<bool> {
    let statements = statements(changes);
    let client = handle.client(handle.default_database()).await?;

    for statement in &statements {
        client.batch_execute(&statement.sql).await?;
    }
    // Recargar hace que los `sighup`/`user` tomen efecto ya; los `postmaster` quedan pendientes.
    client
        .batch_execute("SELECT pg_catalog.pg_reload_conf()")
        .await?;

    // ¿Alguno de los que se cambiaron quedó esperando un reinicio? Un `RESET` no puede dejar nada
    // pendiente, así que solo importan los nombres tocados.
    let names: Vec<&str> = changes.iter().map(SettingChange::name).collect();
    if names.is_empty() {
        return Ok(false);
    }
    let row = client
        .query_one(
            "SELECT coalesce(bool_or(pending_restart), false)
               FROM pg_catalog.pg_settings WHERE name = ANY($1)",
            &[&names],
        )
        .await?;
    Ok(row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sql_of(change: SettingChange) -> String {
        statements(&[change]).remove(0).sql
    }

    #[test]
    fn arma_el_set_con_el_valor_como_literal() {
        assert_eq!(
            sql_of(SettingChange::Set {
                name: "work_mem".into(),
                value: "8MB".into(),
            }),
            "ALTER SYSTEM SET work_mem = '8MB'"
        );
    }

    #[test]
    fn dobla_las_comillas_del_valor() {
        assert_eq!(
            sql_of(SettingChange::Set {
                name: "search_path".into(),
                value: "a'b".into(),
            }),
            "ALTER SYSTEM SET search_path = 'a''b'"
        );
    }

    #[test]
    fn arma_el_reset() {
        assert_eq!(
            sql_of(SettingChange::Reset {
                name: "work_mem".into(),
            }),
            "ALTER SYSTEM RESET work_mem"
        );
    }

    /// El nombre con punto de un parámetro de extensión no se cita: se emite tal cual.
    #[test]
    fn conserva_el_nombre_con_punto() {
        assert_eq!(
            sql_of(SettingChange::Set {
                name: "auto_explain.log_min_duration".into(),
                value: "100".into(),
            }),
            "ALTER SYSTEM SET auto_explain.log_min_duration = '100'"
        );
    }
}
