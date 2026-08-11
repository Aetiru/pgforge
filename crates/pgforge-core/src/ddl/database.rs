//! Crear, cambiar y borrar bases.
//!
//! Es el único módulo de DDL que **no** corre adentro de una transacción, y no por elección:
//! PostgreSQL rechaza `CREATE DATABASE` y `DROP DATABASE` dentro de un bloque transaccional. Por
//! eso [`apply`] ejecuta sentencia por sentencia y, si una falla a mitad de la lista, lo anterior
//! ya quedó hecho. La interfaz manda de a un cambio por vez justamente por eso.
//!
//! El otro detalle que no se puede resolver desde el diálogo: no se puede borrar la base a la que
//! uno está conectado, ni crear una desde una conexión a la que se va a borrar después. [`apply`]
//! elige la base de trabajo con [`working_database`] y no la recibe de afuera, para que ese
//! razonamiento viva en un solo lugar.
//!
//! `WITH (FORCE)` echa a las demás sesiones en vez de fallar con «la base está siendo usada por
//! otros usuarios». Existe desde PostgreSQL 13, que es el piso soportado por el proyecto, así que
//! no hace falta gatearlo por versión.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{quote_ident, role_name};

use serde::{Deserialize, Serialize};

/// Lo que se puede pedir al crear una base. Cada campo vacío lo decide el servidor.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseOptions {
    #[serde(default)]
    pub owner: Option<String>,
    /// De qué base se copia. Sin esto, `template1`.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub lc_collate: Option<String>,
    #[serde(default)]
    pub lc_ctype: Option<String>,
    #[serde(default)]
    pub tablespace: Option<String>,
    /// `-1` es sin límite.
    #[serde(default)]
    pub connection_limit: Option<i32>,
    #[serde(default)]
    pub is_template: Option<bool>,
}

/// Un cambio de base pendiente.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DatabaseChange {
    CreateDatabase {
        name: String,
        options: DatabaseOptions,
    },
    RenameDatabase {
        name: String,
        new_name: String,
    },
    SetDatabaseOwner {
        name: String,
        owner: String,
    },
    SetDatabaseConnectionLimit {
        name: String,
        /// `-1` es sin límite.
        limit: i32,
    },
    /// Impide que se conecte nadie más, para poder borrarla o copiarla sin sorpresas.
    SetDatabaseAllowConnections {
        name: String,
        allow: bool,
    },
    DropDatabase {
        name: String,
        #[serde(default)]
        if_exists: bool,
        /// Echa a las sesiones conectadas en vez de fallar.
        #[serde(default)]
        force: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Escribe un literal de texto, doblando las comillas simples.
///
/// El encoding y los locales son cadenas y no identificadores: van entre comillas simples y por eso
/// no sirve `quote_ident`.
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(changes: &[DatabaseChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &DatabaseChange) -> Result<Statement> {
    match change {
        DatabaseChange::CreateDatabase { name, options } => {
            require_name(name)?;
            let mut parts = Vec::new();

            if let Some(owner) = trimmed(options.owner.as_deref()) {
                parts.push(format!("OWNER = {}", role_name(owner)));
            }
            if let Some(template) = trimmed(options.template.as_deref()) {
                parts.push(format!("TEMPLATE = {}", quote_ident(template)));
            }
            if let Some(encoding) = trimmed(options.encoding.as_deref()) {
                parts.push(format!("ENCODING = {}", literal(encoding)));
            }
            if let Some(collate) = trimmed(options.lc_collate.as_deref()) {
                parts.push(format!("LC_COLLATE = {}", literal(collate)));
            }
            if let Some(ctype) = trimmed(options.lc_ctype.as_deref()) {
                parts.push(format!("LC_CTYPE = {}", literal(ctype)));
            }
            if let Some(tablespace) = trimmed(options.tablespace.as_deref()) {
                parts.push(format!("TABLESPACE = {}", quote_ident(tablespace)));
            }
            if let Some(limit) = options.connection_limit {
                parts.push(format!("CONNECTION LIMIT = {limit}"));
            }
            if let Some(is_template) = options.is_template {
                parts.push(format!("IS_TEMPLATE = {is_template}"));
            }

            let mut sql = format!("CREATE DATABASE {}", quote_ident(name));
            if !parts.is_empty() {
                sql.push_str(&format!("\n    WITH {}", parts.join("\n         ")));
            }
            Ok(statement(sql))
        }
        DatabaseChange::RenameDatabase { name, new_name } => {
            require_name(new_name)?;
            Ok(statement(format!(
                "ALTER DATABASE {} RENAME TO {}",
                quote_ident(name),
                quote_ident(new_name)
            )))
        }
        DatabaseChange::SetDatabaseOwner { name, owner } => {
            require_name(owner)?;
            Ok(statement(format!(
                "ALTER DATABASE {} OWNER TO {}",
                quote_ident(name),
                role_name(owner)
            )))
        }
        DatabaseChange::SetDatabaseConnectionLimit { name, limit } => Ok(statement(format!(
            "ALTER DATABASE {} CONNECTION LIMIT {limit}",
            quote_ident(name)
        ))),
        DatabaseChange::SetDatabaseAllowConnections { name, allow } => Ok(statement(format!(
            "ALTER DATABASE {} ALLOW_CONNECTIONS {allow}",
            quote_ident(name)
        ))),
        DatabaseChange::DropDatabase {
            name,
            if_exists,
            force,
        } => {
            require_name(name)?;
            Ok(statement(format!(
                "DROP DATABASE {}{}{}",
                if *if_exists { "IF EXISTS " } else { "" },
                quote_ident(name),
                if *force { " WITH (FORCE)" } else { "" }
            )))
        }
    }
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn require_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Config("falta el nombre".to_owned()));
    }
    Ok(())
}

/// Qué base tocan los cambios, para no ejecutarlos desde adentro de ella.
fn targets(changes: &[DatabaseChange]) -> Vec<&str> {
    changes
        .iter()
        .map(|change| match change {
            DatabaseChange::CreateDatabase { name, .. }
            | DatabaseChange::RenameDatabase { name, .. }
            | DatabaseChange::SetDatabaseOwner { name, .. }
            | DatabaseChange::SetDatabaseConnectionLimit { name, .. }
            | DatabaseChange::SetDatabaseAllowConnections { name, .. }
            | DatabaseChange::DropDatabase { name, .. } => name.as_str(),
        })
        .collect()
}

/// Desde qué base se ejecutan los cambios.
///
/// No se puede borrar ni renombrar la base a la que uno está conectado, así que si alguno de los
/// cambios apunta a la base por omisión del perfil hay que salir a otra. `postgres` y `template1`
/// existen en cualquier servidor que no las haya borrado a mano; se prueban en ese orden y el
/// error, si ninguna sirve, lo da la conexión.
pub fn working_database<'a>(default: &'a str, changes: &[DatabaseChange]) -> &'a str {
    const FALLBACKS: [&str; 2] = ["postgres", "template1"];

    if !targets(changes).contains(&default) {
        return default;
    }

    FALLBACKS
        .into_iter()
        .find(|candidate| *candidate != default)
        .unwrap_or("template1")
}

/// Aplica los cambios **sin** transacción: `CREATE DATABASE` y `DROP DATABASE` no la admiten.
///
/// La consecuencia es que una lista a medias deja hecho lo anterior. Es la única operación del DDL
/// donde eso pasa, y por eso la interfaz manda un cambio por vez.
pub async fn apply(handle: &ServerHandle, changes: &[DatabaseChange]) -> Result<()> {
    let statements = statements(changes)?;
    let database = working_database(handle.default_database(), changes);
    let client = handle.client(database).await?;

    for statement in &statements {
        client.batch_execute(&statement.sql).await?;
    }

    Ok(())
}

/// Lo que hay que mostrar de una base al abrir «Editar».
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub owner: String,
    pub encoding: String,
    pub collate: String,
    pub ctype: String,
    pub tablespace: String,
    pub connection_limit: i32,
    pub allow_connections: bool,
    pub is_template: bool,
    /// Tamaño en bytes. Es `None` cuando el rol no puede leerlo.
    pub size: Option<i64>,
    pub comment: Option<String>,
}

/// Lee la definición de una base.
pub async fn info(handle: &ServerHandle, name: &str) -> Result<DatabaseInfo> {
    let client = handle.client(handle.default_database()).await?;

    let row = client
        .query_one(
            // `pg_database_size` falla si el rol no tiene `CONNECT` sobre la base, y eso no es
            // motivo para no mostrar el resto: el `CASE` lo evita antes de llamarla.
            "SELECT d.datname::text,
                    pg_catalog.pg_get_userbyid(d.datdba)::text,
                    pg_catalog.pg_encoding_to_char(d.encoding),
                    d.datcollate::text,
                    d.datctype::text,
                    t.spcname::text,
                    d.datconnlimit,
                    d.datallowconn,
                    d.datistemplate,
                    CASE WHEN pg_catalog.has_database_privilege(d.datname, 'CONNECT')
                         THEN pg_catalog.pg_database_size(d.oid) END,
                    pg_catalog.shobj_description(d.oid, 'pg_database')
               FROM pg_catalog.pg_database d
               JOIN pg_catalog.pg_tablespace t ON t.oid = d.dattablespace
              WHERE d.datname = $1",
            &[&name],
        )
        .await?;

    Ok(DatabaseInfo {
        name: row.get(0),
        owner: row.get(1),
        encoding: row.get(2),
        collate: row.get(3),
        ctype: row.get(4),
        tablespace: row.get(5),
        connection_limit: row.get(6),
        allow_connections: row.get(7),
        is_template: row.get(8),
        size: row.get(9),
        comment: row.get(10),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: DatabaseChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn crea_una_base_sin_opciones() {
        let statement = one_statement(DatabaseChange::CreateDatabase {
            name: "ventas".into(),
            options: DatabaseOptions::default(),
        });
        assert_eq!(statement.sql, "CREATE DATABASE ventas");
    }

    #[test]
    fn crea_una_base_con_opciones() {
        let statement = one_statement(DatabaseChange::CreateDatabase {
            name: "ventas".into(),
            options: DatabaseOptions {
                owner: Some("analistas".into()),
                template: Some("template0".into()),
                encoding: Some("UTF8".into()),
                lc_collate: Some("es_AR.UTF-8".into()),
                lc_ctype: Some("es_AR.UTF-8".into()),
                tablespace: None,
                connection_limit: Some(10),
                is_template: Some(false),
            },
        });
        assert_eq!(
            statement.sql,
            concat!(
                "CREATE DATABASE ventas\n",
                "    WITH OWNER = analistas\n",
                "         TEMPLATE = template0\n",
                "         ENCODING = 'UTF8'\n",
                "         LC_COLLATE = 'es_AR.UTF-8'\n",
                "         LC_CTYPE = 'es_AR.UTF-8'\n",
                "         CONNECTION LIMIT = 10\n",
                "         IS_TEMPLATE = false",
            )
        );
    }

    #[test]
    fn una_base_sin_nombre_no_se_genera() {
        assert!(statements(&[DatabaseChange::CreateDatabase {
            name: "   ".into(),
            options: DatabaseOptions::default(),
        }])
        .is_err());
    }

    #[test]
    fn renombra_cambia_dueno_limite_y_conexiones() {
        let statement = one_statement(DatabaseChange::RenameDatabase {
            name: "ventas".into(),
            new_name: "comercial".into(),
        });
        assert_eq!(statement.sql, "ALTER DATABASE ventas RENAME TO comercial");

        let statement = one_statement(DatabaseChange::SetDatabaseOwner {
            name: "ventas".into(),
            owner: "analistas".into(),
        });
        assert_eq!(statement.sql, "ALTER DATABASE ventas OWNER TO analistas");

        let statement = one_statement(DatabaseChange::SetDatabaseConnectionLimit {
            name: "ventas".into(),
            limit: -1,
        });
        assert_eq!(statement.sql, "ALTER DATABASE ventas CONNECTION LIMIT -1");

        let statement = one_statement(DatabaseChange::SetDatabaseAllowConnections {
            name: "ventas".into(),
            allow: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER DATABASE ventas ALLOW_CONNECTIONS false"
        );
    }

    #[test]
    fn borra_con_if_exists_y_con_force() {
        let statement = one_statement(DatabaseChange::DropDatabase {
            name: "ventas".into(),
            if_exists: false,
            force: false,
        });
        assert_eq!(statement.sql, "DROP DATABASE ventas");

        let statement = one_statement(DatabaseChange::DropDatabase {
            name: "ventas".into(),
            if_exists: true,
            force: true,
        });
        assert_eq!(statement.sql, "DROP DATABASE IF EXISTS ventas WITH (FORCE)");
    }

    #[test]
    fn se_conecta_a_otra_base_cuando_el_cambio_toca_la_propia() {
        let borrar_la_propia = [DatabaseChange::DropDatabase {
            name: "ventas".into(),
            if_exists: false,
            force: false,
        }];
        assert_eq!(working_database("ventas", &borrar_la_propia), "postgres");

        // Y si la base por omisión ya es `postgres`, hay que salir a la otra.
        let borrar_postgres = [DatabaseChange::DropDatabase {
            name: "postgres".into(),
            if_exists: false,
            force: false,
        }];
        assert_eq!(working_database("postgres", &borrar_postgres), "template1");
    }

    #[test]
    fn se_queda_en_la_base_por_omision_cuando_el_cambio_toca_otra() {
        let crear = [DatabaseChange::CreateDatabase {
            name: "nueva".into(),
            options: DatabaseOptions::default(),
        }];
        assert_eq!(working_database("ventas", &crear), "ventas");
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = one_statement(DatabaseChange::CreateDatabase {
            name: "mi base".into(),
            options: DatabaseOptions {
                owner: Some("Mi Rol".into()),
                ..DatabaseOptions::default()
            },
        });
        assert_eq!(
            statement.sql,
            "CREATE DATABASE \"mi base\"\n    WITH OWNER = \"Mi Rol\""
        );
    }
}
