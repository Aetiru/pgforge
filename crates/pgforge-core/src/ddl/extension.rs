//! Instalar, actualizar, mover de esquema y quitar extensiones.
//!
//! Una extensión es de la base, no del esquema ni del clúster: cada base tiene las suyas. Las
//! sentencias van por **nombre** (`CREATE/ALTER/DROP EXTENSION nombre`), que es la clave real —no
//! hay un identificador de esquema como en las tablas—.
//!
//! El nombre y el esquema se citan como identificadores; la **versión** se cita como literal de
//! cadena (comillas simples), igual que la contraseña en [`super::role`]: es un valor, suele llevar
//! puntos (`'1.1'`) y no un identificador.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::quote_ident;
use super::table::Statement;

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExtensionChange {
    /// Instala la extensión. `IF NOT EXISTS` evita el error si otra sesión ya la instaló.
    Create {
        name: String,
        schema: Option<String>,
        version: Option<String>,
        cascade: bool,
    },
    /// Actualiza a una versión más nueva. Sin `version` sube a la versión por omisión.
    Update {
        name: String,
        version: Option<String>,
    },
    /// Solo tiene sentido si la extensión es relocatable.
    SetSchema {
        name: String,
        schema: String,
    },
    Drop {
        name: String,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Comillas simples dobladas: el mismo criterio que `role::quote_literal`.
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn statements(changes: &[ExtensionChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &ExtensionChange) -> Result<Statement> {
    match change {
        ExtensionChange::Create {
            name,
            schema,
            version,
            cascade,
        } => {
            let mut sql = format!("CREATE EXTENSION IF NOT EXISTS {}", quote_ident(name));
            if let Some(schema) = schema {
                sql.push_str(&format!(" SCHEMA {}", quote_ident(schema)));
            }
            if let Some(version) = version {
                sql.push_str(&format!(" VERSION {}", quote_literal(version)));
            }
            if *cascade {
                sql.push_str(" CASCADE");
            }
            Ok(statement(sql))
        }
        ExtensionChange::Update { name, version } => {
            let mut sql = format!("ALTER EXTENSION {} UPDATE", quote_ident(name));
            if let Some(version) = version {
                sql.push_str(&format!(" TO {}", quote_literal(version)));
            }
            Ok(statement(sql))
        }
        ExtensionChange::SetSchema { name, schema } => Ok(statement(format!(
            "ALTER EXTENSION {} SET SCHEMA {}",
            quote_ident(name),
            quote_ident(schema)
        ))),
        ExtensionChange::Drop { name, cascade } => Ok(statement(format!(
            "DROP EXTENSION {}{}",
            quote_ident(name),
            if *cascade { " CASCADE" } else { "" }
        ))),
    }
}

/// Aplica los cambios en una sola transacción: mismo molde que `role::apply`.
pub async fn apply(
    handle: &ServerHandle,
    database: &str,
    changes: &[ExtensionChange],
) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Una extensión instalada, para precargar el diálogo de edición y el panel de detalle.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub name: String,
    pub version: String,
    pub schema: String,
    pub comment: Option<String>,
    /// Si la versión instalada se puede mover de esquema. Solo entonces tiene sentido `SetSchema`.
    pub relocatable: bool,
    /// La versión por omisión que ofrece el paquete; puede ser más nueva que la instalada.
    pub default_version: Option<String>,
    /// Todas las versiones que el paquete ofrece, para el selector de "actualizar a".
    pub available_versions: Vec<String>,
}

/// La extensión instalada, por nombre. Junta lo de `pg_extension` (versión y esquema reales) con lo
/// que ofrece el paquete en `pg_available_extension_versions` (si es relocatable, qué versiones hay).
pub async fn extension(handle: &ServerHandle, database: &str, name: &str) -> Result<ExtensionInfo> {
    let client = handle.client(database).await?;
    let row = client
        .query_opt(
            "SELECT e.extversion,
                    n.nspname::text,
                    pg_catalog.obj_description(e.oid, 'pg_extension'),
                    ae.default_version,
                    coalesce(av.relocatable, false)
               FROM pg_catalog.pg_extension e
               JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace
               LEFT JOIN pg_catalog.pg_available_extensions ae ON ae.name = e.extname
               LEFT JOIN pg_catalog.pg_available_extension_versions av
                      ON av.name = e.extname AND av.version = e.extversion
              WHERE e.extname = $1",
            &[&name],
        )
        .await?
        .ok_or_else(|| {
            Error::Config(format!(
                "no hay ninguna extensión instalada llamada «{name}»"
            ))
        })?;

    let versions = client
        .query(
            "SELECT version FROM pg_catalog.pg_available_extension_versions
              WHERE name = $1 ORDER BY version",
            &[&name],
        )
        .await?;

    Ok(ExtensionInfo {
        name: name.to_owned(),
        version: row.get(0),
        schema: row.get(1),
        comment: row.get(2),
        default_version: row.get(3),
        relocatable: row.get(4),
        available_versions: versions.into_iter().map(|row| row.get(0)).collect(),
    })
}

/// Una extensión que el paquete ofrece, esté instalada o no, para el selector al instalar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableExtension {
    pub name: String,
    pub default_version: Option<String>,
    pub installed: bool,
    pub comment: Option<String>,
}

pub async fn available(handle: &ServerHandle, database: &str) -> Result<Vec<AvailableExtension>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT name::text, default_version, installed_version IS NOT NULL, comment
               FROM pg_catalog.pg_available_extensions
              ORDER BY name",
            &[],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| AvailableExtension {
            name: row.get(0),
            default_version: row.get(1),
            installed: row.get(2),
            comment: row.get(3),
        })
        .collect())
}

/// Reconstruye el `CREATE EXTENSION` para el panel de DDL: no existe un `pg_get_extensiondef`, así
/// que se arma con el mismo generador que la vista previa, sobre lo que devuelve el catálogo.
pub fn describe(info: &ExtensionInfo) -> String {
    let statement = one(&ExtensionChange::Create {
        name: info.name.clone(),
        schema: Some(info.schema.clone()),
        version: Some(info.version.clone()),
        cascade: false,
    })
    .expect("CREATE EXTENSION siempre genera una sentencia");
    format!("{};", statement.sql)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: ExtensionChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn instala_una_extension_simple() {
        let statement = one_statement(ExtensionChange::Create {
            name: "pgcrypto".into(),
            schema: None,
            version: None,
            cascade: false,
        });
        assert_eq!(statement.sql, "CREATE EXTENSION IF NOT EXISTS pgcrypto");
    }

    #[test]
    fn instala_con_esquema_version_y_cascade() {
        let statement = one_statement(ExtensionChange::Create {
            name: "postgis".into(),
            schema: Some("gis".into()),
            version: Some("3.4.2".into()),
            cascade: true,
        });
        assert_eq!(
            statement.sql,
            "CREATE EXTENSION IF NOT EXISTS postgis SCHEMA gis VERSION '3.4.2' CASCADE"
        );
    }

    /// Un nombre con guion (`uuid-ossp`) no es un identificador simple y hay que citarlo.
    #[test]
    fn cita_el_nombre_con_simbolos() {
        let statement = one_statement(ExtensionChange::Create {
            name: "uuid-ossp".into(),
            schema: None,
            version: None,
            cascade: false,
        });
        assert_eq!(
            statement.sql,
            "CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\""
        );
    }

    #[test]
    fn actualiza_a_una_version_o_a_la_por_omision() {
        let statement = one_statement(ExtensionChange::Update {
            name: "hstore".into(),
            version: Some("1.8".into()),
        });
        assert_eq!(statement.sql, "ALTER EXTENSION hstore UPDATE TO '1.8'");

        let statement = one_statement(ExtensionChange::Update {
            name: "hstore".into(),
            version: None,
        });
        assert_eq!(statement.sql, "ALTER EXTENSION hstore UPDATE");
    }

    #[test]
    fn cambia_de_esquema() {
        let statement = one_statement(ExtensionChange::SetSchema {
            name: "citext".into(),
            schema: "otro".into(),
        });
        assert_eq!(statement.sql, "ALTER EXTENSION citext SET SCHEMA otro");
    }

    #[test]
    fn quita_con_y_sin_cascade() {
        let statement = one_statement(ExtensionChange::Drop {
            name: "pgcrypto".into(),
            cascade: true,
        });
        assert_eq!(statement.sql, "DROP EXTENSION pgcrypto CASCADE");

        let statement = one_statement(ExtensionChange::Drop {
            name: "pgcrypto".into(),
            cascade: false,
        });
        assert_eq!(statement.sql, "DROP EXTENSION pgcrypto");
    }

    #[test]
    fn describe_reconstruye_el_create() {
        let info = ExtensionInfo {
            name: "pgcrypto".into(),
            version: "1.3".into(),
            schema: "public".into(),
            comment: None,
            relocatable: false,
            default_version: Some("1.3".into()),
            available_versions: vec!["1.3".into()],
        };
        assert_eq!(
            describe(&info),
            "CREATE EXTENSION IF NOT EXISTS pgcrypto SCHEMA public VERSION '1.3';"
        );
    }
}
