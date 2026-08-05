//! Datos externos: wrappers (`FOREIGN DATA WRAPPER`), servidores foráneos (`SERVER`) y mapeos de
//! usuario (`USER MAPPING`).
//!
//! Los tres comparten la cláusula `OPTIONS`, una lista clave-valor: la clave es un identificador y
//! el valor un literal de cadena (comillas simples), igual que en [`super::role`]. Al crear se
//! escriben todas; al alterar se emite `OPTIONS (ADD/SET/DROP …)` según lo que cambió respecto de
//! las que ya tenía.
//!
//! Molde: [`super::role`] y [`super::extension`] —enum de cambios → `statements()` puro →
//! `apply()` en transacción → lectores `*Info` → `describe_*()`—.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{quote_ident, role_name};

// ---------------------------------------------------------------------------
// OPTIONS
// ---------------------------------------------------------------------------

/// El cambio de la lista de opciones respecto de la que ya tenía el objeto: qué se agrega, qué se
/// cambia de valor y qué se quita. La interfaz lo calcula comparando el estado original con el
/// editado, porque Postgres distingue `ADD` (opción nueva) de `SET` (opción existente).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsDelta {
    pub add: Vec<(String, String)>,
    pub set: Vec<(String, String)>,
    pub drop: Vec<String>,
}

impl OptionsDelta {
    fn is_empty(&self) -> bool {
        self.add.is_empty() && self.set.is_empty() && self.drop.is_empty()
    }
}

/// Comillas simples dobladas para un valor de opción: es un dato, no una expresión.
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Un nombre de función (handler/validator), que puede venir calificado por esquema.
fn function_ref(name: &str) -> String {
    name.split('.')
        .map(quote_ident)
        .collect::<Vec<_>>()
        .join(".")
}

/// La cláusula `OPTIONS (…)` de un `CREATE`, o vacía si no hay opciones.
fn create_options(options: &[(String, String)]) -> String {
    if options.is_empty() {
        return String::new();
    }
    let pairs = options
        .iter()
        .map(|(key, value)| format!("{} {}", quote_ident(key), quote_literal(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" OPTIONS ({pairs})")
}

/// La cláusula `OPTIONS (ADD/SET/DROP …)` de un `ALTER`, o vacía si no hay cambios.
fn alter_options(delta: &OptionsDelta) -> String {
    if delta.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for (key, value) in &delta.add {
        parts.push(format!("ADD {} {}", quote_ident(key), quote_literal(value)));
    }
    for (key, value) in &delta.set {
        parts.push(format!("SET {} {}", quote_ident(key), quote_literal(value)));
    }
    for key in &delta.drop {
        parts.push(format!("DROP {}", quote_ident(key)));
    }
    format!(" OPTIONS ({})", parts.join(", "))
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Ejecuta las sentencias ya armadas en una sola transacción. Los tres tipos comparten el aplicador.
pub async fn apply(handle: &ServerHandle, database: &str, statements: &[Statement]) -> Result<()> {
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;
    for statement in statements {
        transaction.batch_execute(&statement.sql).await?;
    }
    transaction.commit().await?;
    Ok(())
}

fn parse_options(raw: Option<Vec<String>>) -> Vec<(String, String)> {
    raw.unwrap_or_default()
        .into_iter()
        .map(|item| match item.split_once('=') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (item, String::new()),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Wrapper (FOREIGN DATA WRAPPER)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FdwChange {
    Create {
        name: String,
        handler: Option<String>,
        validator: Option<String>,
        options: Vec<(String, String)>,
    },
    Alter {
        name: String,
        /// `Some` pone ese handler; `no_handler` lo quita; ninguno lo deja como está.
        handler: Option<String>,
        no_handler: bool,
        validator: Option<String>,
        no_validator: bool,
        options: OptionsDelta,
    },
    Drop {
        name: String,
        cascade: bool,
    },
}

pub fn fdw_statements(changes: &[FdwChange]) -> Result<Vec<Statement>> {
    changes.iter().map(fdw_one).collect()
}

fn fdw_one(change: &FdwChange) -> Result<Statement> {
    match change {
        FdwChange::Create {
            name,
            handler,
            validator,
            options,
        } => {
            let mut sql = format!("CREATE FOREIGN DATA WRAPPER {}", quote_ident(name));
            match handler {
                Some(handler) => sql.push_str(&format!(" HANDLER {}", function_ref(handler))),
                None => sql.push_str(" NO HANDLER"),
            }
            match validator {
                Some(validator) => sql.push_str(&format!(" VALIDATOR {}", function_ref(validator))),
                None => sql.push_str(" NO VALIDATOR"),
            }
            sql.push_str(&create_options(options));
            Ok(statement(sql))
        }
        FdwChange::Alter {
            name,
            handler,
            no_handler,
            validator,
            no_validator,
            options,
        } => {
            let mut sql = format!("ALTER FOREIGN DATA WRAPPER {}", quote_ident(name));
            if let Some(handler) = handler {
                sql.push_str(&format!(" HANDLER {}", function_ref(handler)));
            } else if *no_handler {
                sql.push_str(" NO HANDLER");
            }
            if let Some(validator) = validator {
                sql.push_str(&format!(" VALIDATOR {}", function_ref(validator)));
            } else if *no_validator {
                sql.push_str(" NO VALIDATOR");
            }
            sql.push_str(&alter_options(options));

            if sql.ends_with(&quote_ident(name)) {
                return Err(Error::Config(
                    "no hay nada que cambiar en el wrapper".to_owned(),
                ));
            }
            Ok(statement(sql))
        }
        FdwChange::Drop { name, cascade } => Ok(statement(format!(
            "DROP FOREIGN DATA WRAPPER {}{}",
            quote_ident(name),
            if *cascade { " CASCADE" } else { "" }
        ))),
    }
}

// ---------------------------------------------------------------------------
// Servidor foráneo (SERVER)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ServerChange {
    Create {
        name: String,
        fdw: String,
        server_type: Option<String>,
        version: Option<String>,
        options: Vec<(String, String)>,
    },
    /// El `TYPE` de un servidor no se puede alterar, solo la versión y las opciones.
    Alter {
        name: String,
        version: Option<String>,
        options: OptionsDelta,
    },
    Drop {
        name: String,
        cascade: bool,
    },
}

pub fn server_statements(changes: &[ServerChange]) -> Result<Vec<Statement>> {
    changes.iter().map(server_one).collect()
}

fn server_one(change: &ServerChange) -> Result<Statement> {
    match change {
        ServerChange::Create {
            name,
            fdw,
            server_type,
            version,
            options,
        } => {
            let mut sql = format!("CREATE SERVER {}", quote_ident(name));
            if let Some(server_type) = server_type {
                sql.push_str(&format!(" TYPE {}", quote_literal(server_type)));
            }
            if let Some(version) = version {
                sql.push_str(&format!(" VERSION {}", quote_literal(version)));
            }
            sql.push_str(&format!(" FOREIGN DATA WRAPPER {}", quote_ident(fdw)));
            sql.push_str(&create_options(options));
            Ok(statement(sql))
        }
        ServerChange::Alter {
            name,
            version,
            options,
        } => {
            let mut sql = format!("ALTER SERVER {}", quote_ident(name));
            if let Some(version) = version {
                sql.push_str(&format!(" VERSION {}", quote_literal(version)));
            }
            sql.push_str(&alter_options(options));

            if version.is_none() && options.is_empty() {
                return Err(Error::Config(
                    "no hay nada que cambiar en el servidor".to_owned(),
                ));
            }
            Ok(statement(sql))
        }
        ServerChange::Drop { name, cascade } => Ok(statement(format!(
            "DROP SERVER {}{}",
            quote_ident(name),
            if *cascade { " CASCADE" } else { "" }
        ))),
    }
}

// ---------------------------------------------------------------------------
// Mapeo de usuario (USER MAPPING)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UserMappingChange {
    Create {
        server: String,
        user: String,
        options: Vec<(String, String)>,
    },
    Alter {
        server: String,
        user: String,
        options: OptionsDelta,
    },
    Drop {
        server: String,
        user: String,
    },
}

pub fn user_mapping_statements(changes: &[UserMappingChange]) -> Result<Vec<Statement>> {
    changes.iter().map(user_mapping_one).collect()
}

fn user_mapping_one(change: &UserMappingChange) -> Result<Statement> {
    match change {
        UserMappingChange::Create {
            server,
            user,
            options,
        } => Ok(statement(format!(
            "CREATE USER MAPPING FOR {} SERVER {}{}",
            role_name(user),
            quote_ident(server),
            create_options(options)
        ))),
        UserMappingChange::Alter {
            server,
            user,
            options,
        } => {
            if options.is_empty() {
                return Err(Error::Config(
                    "no hay nada que cambiar en el mapeo".to_owned(),
                ));
            }
            Ok(statement(format!(
                "ALTER USER MAPPING FOR {} SERVER {}{}",
                role_name(user),
                quote_ident(server),
                alter_options(options)
            )))
        }
        UserMappingChange::Drop { server, user } => Ok(statement(format!(
            "DROP USER MAPPING FOR {} SERVER {}",
            role_name(user),
            quote_ident(server)
        ))),
    }
}

// ---------------------------------------------------------------------------
// Lectores
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdwInfo {
    pub name: String,
    pub handler: Option<String>,
    pub validator: Option<String>,
    pub options: Vec<(String, String)>,
    pub owner: String,
}

pub async fn fdw_info(handle: &ServerHandle, database: &str, name: &str) -> Result<FdwInfo> {
    let client = handle.client(database).await?;
    let row = client
        .query_opt(
            "SELECT h.proname::text,
                    v.proname::text,
                    w.fdwoptions,
                    pg_catalog.pg_get_userbyid(w.fdwowner)::text
               FROM pg_catalog.pg_foreign_data_wrapper w
               LEFT JOIN pg_catalog.pg_proc h ON h.oid = w.fdwhandler
               LEFT JOIN pg_catalog.pg_proc v ON v.oid = w.fdwvalidator
              WHERE w.fdwname = $1",
            &[&name],
        )
        .await?
        .ok_or_else(|| Error::Config(format!("no existe el wrapper «{name}»")))?;

    Ok(FdwInfo {
        name: name.to_owned(),
        handler: row.get(0),
        validator: row.get(1),
        options: parse_options(row.get(2)),
        owner: row.get(3),
    })
}

/// Los wrappers disponibles, para el selector al crear un servidor.
pub async fn available_fdws(handle: &ServerHandle, database: &str) -> Result<Vec<String>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT fdwname::text FROM pg_catalog.pg_foreign_data_wrapper ORDER BY fdwname",
            &[],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub fdw: String,
    pub server_type: Option<String>,
    pub version: Option<String>,
    pub options: Vec<(String, String)>,
    pub owner: String,
}

pub async fn server_info(handle: &ServerHandle, database: &str, name: &str) -> Result<ServerInfo> {
    let client = handle.client(database).await?;
    let row = client
        .query_opt(
            "SELECT w.fdwname::text,
                    s.srvtype,
                    s.srvversion,
                    s.srvoptions,
                    pg_catalog.pg_get_userbyid(s.srvowner)::text
               FROM pg_catalog.pg_foreign_server s
               JOIN pg_catalog.pg_foreign_data_wrapper w ON w.oid = s.srvfdw
              WHERE s.srvname = $1",
            &[&name],
        )
        .await?
        .ok_or_else(|| Error::Config(format!("no existe el servidor foráneo «{name}»")))?;

    Ok(ServerInfo {
        name: name.to_owned(),
        fdw: row.get(0),
        server_type: row.get(1),
        version: row.get(2),
        options: parse_options(row.get(3)),
        owner: row.get(4),
    })
}

/// Un mapeo de usuario de un servidor. `options` es `None` cuando el rol conectado no puede verlas
/// (Postgres las oculta a quien no es dueño del mapeo ni superusuario).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMapping {
    pub user: String,
    pub options: Option<Vec<(String, String)>>,
}

pub async fn user_mappings(
    handle: &ServerHandle,
    database: &str,
    server: &str,
) -> Result<Vec<UserMapping>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT usename::text, umoptions
               FROM pg_catalog.pg_user_mappings
              WHERE srvname = $1
              ORDER BY usename",
            &[&server],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            // usename es NULL en el mapeo PUBLIC (umuser = 0).
            let user: Option<String> = row.get(0);
            let raw: Option<Vec<String>> = row.get(1);
            UserMapping {
                user: user.unwrap_or_else(|| "PUBLIC".to_owned()),
                options: raw.map(|raw| parse_options(Some(raw))),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// DDL para el panel de detalle
// ---------------------------------------------------------------------------

pub fn describe_fdw(info: &FdwInfo) -> String {
    let statement = fdw_one(&FdwChange::Create {
        name: info.name.clone(),
        handler: info.handler.clone(),
        validator: info.validator.clone(),
        options: info.options.clone(),
    })
    .expect("CREATE FOREIGN DATA WRAPPER siempre genera una sentencia");
    format!("{};", statement.sql)
}

pub fn describe_server(info: &ServerInfo) -> String {
    let statement = server_one(&ServerChange::Create {
        name: info.name.clone(),
        fdw: info.fdw.clone(),
        server_type: info.server_type.clone(),
        version: info.version.clone(),
        options: info.options.clone(),
    })
    .expect("CREATE SERVER siempre genera una sentencia");
    format!("{};", statement.sql)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fdw(change: FdwChange) -> String {
        fdw_statements(&[change]).unwrap().remove(0).sql
    }
    fn server(change: ServerChange) -> String {
        server_statements(&[change]).unwrap().remove(0).sql
    }
    fn mapping(change: UserMappingChange) -> String {
        user_mapping_statements(&[change]).unwrap().remove(0).sql
    }

    #[test]
    fn crea_un_wrapper_con_handler_y_opciones() {
        let sql = fdw(FdwChange::Create {
            name: "mi_fdw".into(),
            handler: Some("postgres_fdw_handler".into()),
            validator: Some("postgres_fdw_validator".into()),
            options: vec![],
        });
        assert_eq!(
            sql,
            "CREATE FOREIGN DATA WRAPPER mi_fdw HANDLER postgres_fdw_handler VALIDATOR postgres_fdw_validator"
        );
    }

    #[test]
    fn un_wrapper_sin_handler_lo_dice() {
        let sql = fdw(FdwChange::Create {
            name: "vacio".into(),
            handler: None,
            validator: None,
            options: vec![],
        });
        assert_eq!(
            sql,
            "CREATE FOREIGN DATA WRAPPER vacio NO HANDLER NO VALIDATOR"
        );
    }

    #[test]
    fn crea_un_servidor_con_tipo_version_y_opciones() {
        let sql = server(ServerChange::Create {
            name: "remoto".into(),
            fdw: "postgres_fdw".into(),
            server_type: None,
            version: Some("16".into()),
            options: vec![
                ("host".into(), "10.0.0.1".into()),
                ("port".into(), "5432".into()),
                ("dbname".into(), "ventas".into()),
            ],
        });
        assert_eq!(
            sql,
            "CREATE SERVER remoto VERSION '16' FOREIGN DATA WRAPPER postgres_fdw \
             OPTIONS (host '10.0.0.1', port '5432', dbname 'ventas')"
        );
    }

    #[test]
    fn altera_las_opciones_de_un_servidor_con_add_set_drop() {
        let sql = server(ServerChange::Alter {
            name: "remoto".into(),
            version: None,
            options: OptionsDelta {
                add: vec![("sslmode".into(), "require".into())],
                set: vec![("host".into(), "10.0.0.2".into())],
                drop: vec!["port".into()],
            },
        });
        assert_eq!(
            sql,
            "ALTER SERVER remoto OPTIONS (ADD sslmode 'require', SET host '10.0.0.2', DROP port)"
        );
    }

    #[test]
    fn alterar_un_servidor_sin_cambios_es_un_error() {
        assert!(server_statements(&[ServerChange::Alter {
            name: "remoto".into(),
            version: None,
            options: OptionsDelta::default(),
        }])
        .is_err());
    }

    #[test]
    fn crea_un_mapeo_para_un_rol_y_para_public() {
        let sql = mapping(UserMappingChange::Create {
            server: "remoto".into(),
            user: "ana".into(),
            options: vec![
                ("user".into(), "remoto_user".into()),
                ("password".into(), "se'creta".into()),
            ],
        });
        assert_eq!(
            sql,
            "CREATE USER MAPPING FOR ana SERVER remoto OPTIONS (\"user\" 'remoto_user', password 'se''creta')"
        );

        let sql = mapping(UserMappingChange::Create {
            server: "remoto".into(),
            user: "public".into(),
            options: vec![],
        });
        assert_eq!(sql, "CREATE USER MAPPING FOR PUBLIC SERVER remoto");
    }

    #[test]
    fn quita_con_cascade_y_sin_el() {
        assert_eq!(
            server(ServerChange::Drop {
                name: "remoto".into(),
                cascade: true
            }),
            "DROP SERVER remoto CASCADE"
        );
        assert_eq!(
            mapping(UserMappingChange::Drop {
                server: "remoto".into(),
                user: "ana".into()
            }),
            "DROP USER MAPPING FOR ana SERVER remoto"
        );
    }
}
