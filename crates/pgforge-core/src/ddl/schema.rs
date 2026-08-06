//! Crear, renombrar, cambiar de dueño y borrar esquemas.
//!
//! Un esquema es casi solo un nombre y un dueño, así que este módulo es corto. Está aparte igual
//! porque es el único lugar donde vive el `CREATE SCHEMA` de la aplicación: antes había uno suelto
//! adentro de la generación de DDL de lectura, que no se podía ejecutar desde ningún lado.
//!
//! `DROP SCHEMA` sin `CASCADE` falla si el esquema tiene algo adentro, y eso es a propósito: la
//! interfaz tiene que preguntar antes de llevarse las tablas puestas.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{quote_ident, role_name};

use serde::Deserialize;

/// Un cambio de esquema pendiente.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SchemaChange {
    CreateSchema {
        name: String,
        /// Vacío deja como dueño al rol conectado.
        #[serde(default)]
        authorization: Option<String>,
        #[serde(default)]
        if_not_exists: bool,
    },
    RenameSchema {
        name: String,
        new_name: String,
    },
    SetSchemaOwner {
        name: String,
        owner: String,
    },
    DropSchema {
        name: String,
        #[serde(default)]
        if_exists: bool,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(changes: &[SchemaChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &SchemaChange) -> Result<Statement> {
    match change {
        SchemaChange::CreateSchema {
            name,
            authorization,
            if_not_exists,
        } => {
            require_name(name)?;
            let owner = match authorization.as_deref().map(str::trim) {
                Some(owner) if !owner.is_empty() => format!(" AUTHORIZATION {}", role_name(owner)),
                _ => String::new(),
            };
            Ok(statement(format!(
                "CREATE SCHEMA {}{}{owner}",
                if *if_not_exists { "IF NOT EXISTS " } else { "" },
                quote_ident(name)
            )))
        }
        SchemaChange::RenameSchema { name, new_name } => {
            require_name(new_name)?;
            Ok(statement(format!(
                "ALTER SCHEMA {} RENAME TO {}",
                quote_ident(name),
                quote_ident(new_name)
            )))
        }
        SchemaChange::SetSchemaOwner { name, owner } => {
            require_name(owner)?;
            Ok(statement(format!(
                "ALTER SCHEMA {} OWNER TO {}",
                quote_ident(name),
                role_name(owner)
            )))
        }
        SchemaChange::DropSchema {
            name,
            if_exists,
            cascade,
        } => {
            require_name(name)?;
            Ok(statement(format!(
                "DROP SCHEMA {}{}{}",
                if *if_exists { "IF EXISTS " } else { "" },
                quote_ident(name),
                if *cascade { " CASCADE" } else { "" }
            )))
        }
    }
}

fn require_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Config("falta el nombre".to_owned()));
    }
    Ok(())
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[SchemaChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: SchemaChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn crea_un_esquema_con_y_sin_dueno() {
        let statement = one_statement(SchemaChange::CreateSchema {
            name: "ventas".into(),
            authorization: None,
            if_not_exists: false,
        });
        assert_eq!(statement.sql, "CREATE SCHEMA ventas");

        let statement = one_statement(SchemaChange::CreateSchema {
            name: "ventas".into(),
            authorization: Some("analistas".into()),
            if_not_exists: true,
        });
        assert_eq!(
            statement.sql,
            "CREATE SCHEMA IF NOT EXISTS ventas AUTHORIZATION analistas"
        );
    }

    #[test]
    fn current_user_no_se_cita() {
        let statement = one_statement(SchemaChange::CreateSchema {
            name: "ventas".into(),
            authorization: Some("current_user".into()),
            if_not_exists: false,
        });
        assert_eq!(
            statement.sql,
            "CREATE SCHEMA ventas AUTHORIZATION CURRENT_USER"
        );
    }

    #[test]
    fn un_esquema_sin_nombre_no_se_genera() {
        assert!(statements(&[SchemaChange::CreateSchema {
            name: "  ".into(),
            authorization: None,
            if_not_exists: false,
        }])
        .is_err());
    }

    #[test]
    fn renombra_y_cambia_de_dueno() {
        let statement = one_statement(SchemaChange::RenameSchema {
            name: "ventas".into(),
            new_name: "comercial".into(),
        });
        assert_eq!(statement.sql, "ALTER SCHEMA ventas RENAME TO comercial");

        let statement = one_statement(SchemaChange::SetSchemaOwner {
            name: "ventas".into(),
            owner: "analistas".into(),
        });
        assert_eq!(statement.sql, "ALTER SCHEMA ventas OWNER TO analistas");
    }

    #[test]
    fn borra_con_y_sin_cascade() {
        let statement = one_statement(SchemaChange::DropSchema {
            name: "ventas".into(),
            if_exists: false,
            cascade: false,
        });
        assert_eq!(statement.sql, "DROP SCHEMA ventas");

        let statement = one_statement(SchemaChange::DropSchema {
            name: "ventas".into(),
            if_exists: true,
            cascade: true,
        });
        assert_eq!(statement.sql, "DROP SCHEMA IF EXISTS ventas CASCADE");
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = one_statement(SchemaChange::CreateSchema {
            name: "mi esquema".into(),
            authorization: Some("Mi Rol".into()),
            if_not_exists: false,
        });
        assert_eq!(
            statement.sql,
            "CREATE SCHEMA \"mi esquema\" AUTHORIZATION \"Mi Rol\""
        );
    }
}
