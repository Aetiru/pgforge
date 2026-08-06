//! Crear, cambiar y borrar dominios.
//!
//! Un dominio es un tipo base con reglas encima: `NOT NULL`, un `DEFAULT` y restricciones `CHECK`
//! con nombre. Vive aparte de [`crate::ddl::types`] porque, aunque los dos son `pg_type`, no
//! comparten ni una cláusula: acá no hay valores ni campos, y allá no hay restricciones.
//!
//! El tipo base, el `DEFAULT` y la expresión de cada `CHECK` van **crudos**, igual que en una
//! columna: no se pueden parametrizar en DDL, los valida el servidor al ejecutar, y es la misma
//! frontera de confianza que el editor de consultas.
//!
//! `VALIDATE CONSTRAINT` existe para el caso en que la restricción se agregó como `NOT VALID`: las
//! filas que ya estaban quedan sin revisar hasta que alguien lo pida. Agregar directamente una
//! restricción válida sobre un dominio con datos hace que el servidor recorra **todas** las
//! columnas que lo usan, y eso sobre una tabla grande no es gratis.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident, role_name};

use serde::{Deserialize, Serialize};

/// Una restricción `CHECK` de un dominio.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainConstraint {
    /// Vacío deja que el servidor lo nombre.
    #[serde(default)]
    pub name: Option<String>,
    /// La expresión, cruda. `VALUE` es el valor que se está validando.
    pub check: String,
    /// `NOT VALID`: no revisa lo que ya está guardado.
    #[serde(default)]
    pub not_valid: bool,
}

/// Un cambio de dominio pendiente.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DomainChange {
    CreateDomain {
        schema: String,
        name: String,
        /// El tipo base, crudo.
        data_type: String,
        #[serde(default)]
        collation: Option<String>,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        not_null: bool,
        #[serde(default)]
        constraints: Vec<DomainConstraint>,
    },
    /// `None` quita el `DEFAULT`.
    SetDomainDefault {
        schema: String,
        name: String,
        #[serde(default)]
        default: Option<String>,
    },
    SetDomainNotNull {
        schema: String,
        name: String,
        not_null: bool,
    },
    AddDomainConstraint {
        schema: String,
        name: String,
        constraint: DomainConstraint,
    },
    /// Revisa lo que ya estaba guardado, para una restricción agregada como `NOT VALID`.
    ValidateDomainConstraint {
        schema: String,
        name: String,
        constraint: String,
    },
    DropDomainConstraint {
        schema: String,
        name: String,
        constraint: String,
        #[serde(default)]
        if_exists: bool,
        cascade: bool,
    },
    RenameDomain {
        schema: String,
        name: String,
        new_name: String,
    },
    SetDomainSchema {
        schema: String,
        name: String,
        new_schema: String,
    },
    SetDomainOwner {
        schema: String,
        name: String,
        owner: String,
    },
    DropDomain {
        schema: String,
        name: String,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

fn constraint_sql(constraint: &DomainConstraint) -> Result<String> {
    let check = constraint.check.trim();
    if check.is_empty() {
        return Err(Error::Config(
            "una restricción necesita una expresión".to_owned(),
        ));
    }

    let named = match constraint.name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => format!("CONSTRAINT {} ", quote_ident(name)),
        _ => String::new(),
    };

    Ok(format!(
        "{named}CHECK ({check}){}",
        if constraint.not_valid { " NOT VALID" } else { "" }
    ))
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(changes: &[DomainChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &DomainChange) -> Result<Statement> {
    match change {
        DomainChange::CreateDomain {
            schema,
            name,
            data_type,
            collation,
            default,
            not_null,
            constraints,
        } => {
            require_name(name)?;
            let data_type = data_type.trim();
            if data_type.is_empty() {
                return Err(Error::Config(
                    "un dominio necesita un tipo base".to_owned(),
                ));
            }

            let mut sql = format!("CREATE DOMAIN {} AS {data_type}", qualified(schema, name));

            if let Some(collation) = collation.as_deref().map(str::trim) {
                if !collation.is_empty() {
                    sql.push_str(&format!(" COLLATE {}", quote_ident(collation)));
                }
            }

            if let Some(default) = default.as_deref().map(str::trim) {
                if !default.is_empty() {
                    sql.push_str(&format!("\n    DEFAULT {default}"));
                }
            }

            if *not_null {
                sql.push_str("\n    NOT NULL");
            }

            for constraint in constraints {
                sql.push_str(&format!("\n    {}", constraint_sql(constraint)?));
            }

            Ok(statement(sql))
        }
        DomainChange::SetDomainDefault {
            schema,
            name,
            default,
        } => {
            let clause = match default.as_deref().map(str::trim) {
                Some(default) if !default.is_empty() => format!("SET DEFAULT {default}"),
                _ => "DROP DEFAULT".to_owned(),
            };
            Ok(statement(format!(
                "ALTER DOMAIN {} {clause}",
                qualified(schema, name)
            )))
        }
        DomainChange::SetDomainNotNull {
            schema,
            name,
            not_null,
        } => Ok(statement(format!(
            "ALTER DOMAIN {} {} NOT NULL",
            qualified(schema, name),
            if *not_null { "SET" } else { "DROP" }
        ))),
        DomainChange::AddDomainConstraint {
            schema,
            name,
            constraint,
        } => Ok(statement(format!(
            "ALTER DOMAIN {} ADD {}",
            qualified(schema, name),
            constraint_sql(constraint)?
        ))),
        DomainChange::ValidateDomainConstraint {
            schema,
            name,
            constraint,
        } => {
            require_name(constraint)?;
            Ok(statement(format!(
                "ALTER DOMAIN {} VALIDATE CONSTRAINT {}",
                qualified(schema, name),
                quote_ident(constraint)
            )))
        }
        DomainChange::DropDomainConstraint {
            schema,
            name,
            constraint,
            if_exists,
            cascade,
        } => {
            require_name(constraint)?;
            Ok(statement(format!(
                "ALTER DOMAIN {} DROP CONSTRAINT {}{}{}",
                qualified(schema, name),
                if *if_exists { "IF EXISTS " } else { "" },
                quote_ident(constraint),
                if *cascade { " CASCADE" } else { "" }
            )))
        }
        DomainChange::RenameDomain {
            schema,
            name,
            new_name,
        } => {
            require_name(new_name)?;
            Ok(statement(format!(
                "ALTER DOMAIN {} RENAME TO {}",
                qualified(schema, name),
                quote_ident(new_name)
            )))
        }
        DomainChange::SetDomainSchema {
            schema,
            name,
            new_schema,
        } => {
            require_name(new_schema)?;
            Ok(statement(format!(
                "ALTER DOMAIN {} SET SCHEMA {}",
                qualified(schema, name),
                quote_ident(new_schema)
            )))
        }
        DomainChange::SetDomainOwner {
            schema,
            name,
            owner,
        } => {
            require_name(owner)?;
            Ok(statement(format!(
                "ALTER DOMAIN {} OWNER TO {}",
                qualified(schema, name),
                role_name(owner)
            )))
        }
        DomainChange::DropDomain {
            schema,
            name,
            cascade,
        } => Ok(statement(format!(
            "DROP DOMAIN {}{}",
            qualified(schema, name),
            if *cascade { " CASCADE" } else { "" }
        ))),
    }
}

fn require_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Config("falta el nombre".to_owned()));
    }
    Ok(())
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[DomainChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Lo que hay que mostrar de un dominio al abrir «Editar».
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainInfo {
    pub schema: String,
    pub name: String,
    pub owner: String,
    pub data_type: String,
    pub collation: Option<String>,
    pub default: Option<String>,
    pub not_null: bool,
    pub constraints: Vec<DomainConstraint>,
    pub comment: Option<String>,
}

/// Lee la definición de un dominio.
pub async fn info(handle: &ServerHandle, database: &str, oid: u32) -> Result<DomainInfo> {
    let client = handle.client(database).await?;

    let row = client
        .query_one(
            "SELECT n.nspname::text,
                    t.typname::text,
                    pg_catalog.pg_get_userbyid(t.typowner)::text,
                    pg_catalog.format_type(t.typbasetype, t.typtypmod),
                    co.collname::text,
                    t.typdefault,
                    t.typnotnull,
                    pg_catalog.obj_description(t.oid, 'pg_type')
               FROM pg_catalog.pg_type t
               JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
          LEFT JOIN pg_catalog.pg_collation co ON co.oid = t.typcollation
              WHERE t.oid = $1 AND t.typtype = 'd'",
            &[&oid],
        )
        .await?;

    // `pg_get_constraintdef` devuelve la restricción entera («CHECK ((VALUE > 0)) NOT VALID»); se
    // le saca el envoltorio para poder volver a mostrarla en el mismo campo donde se escribió.
    let constraints = client
        .query(
            "SELECT c.conname::text,
                    pg_catalog.pg_get_constraintdef(c.oid),
                    c.convalidated
               FROM pg_catalog.pg_constraint c
              WHERE c.contypid = $1
              ORDER BY c.conname",
            &[&oid],
        )
        .await?
        .into_iter()
        .map(|row| {
            let definition: String = row.get(1);
            let validated: bool = row.get(2);
            DomainConstraint {
                name: Some(row.get(0)),
                check: check_body(&definition),
                not_valid: !validated,
            }
        })
        .collect();

    Ok(DomainInfo {
        schema: row.get(0),
        name: row.get(1),
        owner: row.get(2),
        data_type: row.get(3),
        collation: row.get(4),
        default: row.get(5),
        not_null: row.get(6),
        constraints,
        comment: row.get(7),
    })
}

/// Saca el `CHECK (…)` de alrededor de la expresión que devuelve `pg_get_constraintdef`.
///
/// Si el texto no tiene la forma esperada se devuelve tal cual: mostrar de más es mejor que
/// recortar mal una expresión que después se va a ejecutar.
fn check_body(definition: &str) -> String {
    let trimmed = definition.trim();
    let trimmed = trimmed.strip_suffix("NOT VALID").unwrap_or(trimmed).trim();

    match trimmed.strip_prefix("CHECK (") {
        Some(rest) => match rest.strip_suffix(')') {
            Some(body) => body.trim().to_owned(),
            None => trimmed.to_owned(),
        },
        None => trimmed.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: DomainChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn crea_un_dominio_simple() {
        let statement = one_statement(DomainChange::CreateDomain {
            schema: "public".into(),
            name: "positivo".into(),
            data_type: "integer".into(),
            collation: None,
            default: None,
            not_null: false,
            constraints: vec![],
        });
        assert_eq!(statement.sql, "CREATE DOMAIN public.positivo AS integer");
    }

    #[test]
    fn crea_un_dominio_con_todo() {
        let statement = one_statement(DomainChange::CreateDomain {
            schema: "public".into(),
            name: "correo".into(),
            data_type: "text".into(),
            collation: Some("es_AR".into()),
            default: Some("''".into()),
            not_null: true,
            constraints: vec![DomainConstraint {
                name: Some("correo_valido".into()),
                check: "VALUE ~ '@'".into(),
                not_valid: false,
            }],
        });
        assert_eq!(
            statement.sql,
            "CREATE DOMAIN public.correo AS text COLLATE \"es_AR\"\n    DEFAULT ''\n    NOT NULL\
             \n    CONSTRAINT correo_valido CHECK (VALUE ~ '@')"
        );
    }

    #[test]
    fn un_dominio_sin_tipo_base_no_se_genera() {
        assert!(statements(&[DomainChange::CreateDomain {
            schema: "public".into(),
            name: "x".into(),
            data_type: "  ".into(),
            collation: None,
            default: None,
            not_null: false,
            constraints: vec![],
        }])
        .is_err());
    }

    #[test]
    fn una_restriccion_sin_expresion_no_se_genera() {
        assert!(statements(&[DomainChange::AddDomainConstraint {
            schema: "public".into(),
            name: "correo".into(),
            constraint: DomainConstraint {
                name: None,
                check: "   ".into(),
                not_valid: false,
            },
        }])
        .is_err());
    }

    #[test]
    fn pone_y_saca_el_default() {
        let statement = one_statement(DomainChange::SetDomainDefault {
            schema: "public".into(),
            name: "positivo".into(),
            default: Some("0".into()),
        });
        assert_eq!(
            statement.sql,
            "ALTER DOMAIN public.positivo SET DEFAULT 0"
        );

        let statement = one_statement(DomainChange::SetDomainDefault {
            schema: "public".into(),
            name: "positivo".into(),
            default: None,
        });
        assert_eq!(statement.sql, "ALTER DOMAIN public.positivo DROP DEFAULT");
    }

    #[test]
    fn pone_y_saca_el_not_null() {
        let statement = one_statement(DomainChange::SetDomainNotNull {
            schema: "public".into(),
            name: "positivo".into(),
            not_null: true,
        });
        assert_eq!(
            statement.sql,
            "ALTER DOMAIN public.positivo SET NOT NULL"
        );

        let statement = one_statement(DomainChange::SetDomainNotNull {
            schema: "public".into(),
            name: "positivo".into(),
            not_null: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER DOMAIN public.positivo DROP NOT NULL"
        );
    }

    #[test]
    fn agrega_valida_y_borra_una_restriccion() {
        let statement = one_statement(DomainChange::AddDomainConstraint {
            schema: "public".into(),
            name: "positivo".into(),
            constraint: DomainConstraint {
                name: Some("mayor_que_cero".into()),
                check: "VALUE > 0".into(),
                not_valid: true,
            },
        });
        assert_eq!(
            statement.sql,
            "ALTER DOMAIN public.positivo ADD CONSTRAINT mayor_que_cero CHECK (VALUE > 0) NOT VALID"
        );

        let statement = one_statement(DomainChange::ValidateDomainConstraint {
            schema: "public".into(),
            name: "positivo".into(),
            constraint: "mayor_que_cero".into(),
        });
        assert_eq!(
            statement.sql,
            "ALTER DOMAIN public.positivo VALIDATE CONSTRAINT mayor_que_cero"
        );

        let statement = one_statement(DomainChange::DropDomainConstraint {
            schema: "public".into(),
            name: "positivo".into(),
            constraint: "mayor_que_cero".into(),
            if_exists: true,
            cascade: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER DOMAIN public.positivo DROP CONSTRAINT IF EXISTS mayor_que_cero"
        );
    }

    #[test]
    fn una_restriccion_sin_nombre_la_nombra_el_servidor() {
        let statement = one_statement(DomainChange::AddDomainConstraint {
            schema: "public".into(),
            name: "positivo".into(),
            constraint: DomainConstraint {
                name: None,
                check: "VALUE > 0".into(),
                not_valid: false,
            },
        });
        assert_eq!(
            statement.sql,
            "ALTER DOMAIN public.positivo ADD CHECK (VALUE > 0)"
        );
    }

    #[test]
    fn borra_con_y_sin_cascade() {
        let statement = one_statement(DomainChange::DropDomain {
            schema: "public".into(),
            name: "positivo".into(),
            cascade: true,
        });
        assert_eq!(statement.sql, "DROP DOMAIN public.positivo CASCADE");
    }

    #[test]
    fn le_saca_el_envoltorio_a_la_definicion_del_servidor() {
        assert_eq!(check_body("CHECK ((VALUE > 0))"), "(VALUE > 0)");
        assert_eq!(check_body("CHECK ((VALUE > 0)) NOT VALID"), "(VALUE > 0)");
        // Sin la forma esperada se devuelve tal cual: recortar mal sería peor que mostrar de más.
        assert_eq!(check_body("algo raro"), "algo raro");
    }
}
