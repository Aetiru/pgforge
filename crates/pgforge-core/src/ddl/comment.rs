//! `COMMENT ON`: la documentación que vive adentro de la base.
//!
//! Es transversal y no de un objeto: la sentencia es siempre `COMMENT ON <clase> <nombre> IS
//! <texto>`, y lo único que cambia entre una tabla y un rol es cómo se escribe ese nombre. Por eso
//! hay un solo módulo con un enum de destinos, en vez de un cambio de comentario repetido en cada
//! módulo de DDL.
//!
//! Quitar el comentario es `IS NULL`, no `IS ''`: la cadena vacía deja un comentario vacío, que en
//! el árbol se ve como un objeto documentado con nada adentro. Por eso [`CommentChange::comment`]
//! es un `Option` y una cadena en blanco se trata como borrar.
//!
//! El texto sí se puede escapar —es un literal, no un identificador—, así que acá no hay ninguna
//! frontera de confianza que negociar: se dobla la comilla simple y listo.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident, role_name};

use serde::{Deserialize, Serialize};

/// Sobre qué objeto se comenta.
///
/// No están todos los que PostgreSQL admite, sino los que este cliente muestra en el árbol: agregar
/// uno es agregar una variante y su línea en [`target_sql`].
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CommentTarget {
    Table {
        schema: String,
        name: String,
    },
    Column {
        schema: String,
        table: String,
        column: String,
    },
    View {
        schema: String,
        name: String,
    },
    MaterializedView {
        schema: String,
        name: String,
    },
    ForeignTable {
        schema: String,
        name: String,
    },
    Sequence {
        schema: String,
        name: String,
    },
    Index {
        schema: String,
        name: String,
    },
    Type {
        schema: String,
        name: String,
    },
    Domain {
        schema: String,
        name: String,
    },
    Schema {
        name: String,
    },
    Database {
        name: String,
    },
    Role {
        name: String,
    },
    Extension {
        name: String,
    },
    /// La firma va cruda: `COMMENT ON FUNCTION` necesita los tipos de los argumentos para
    /// distinguir entre sobrecargas, y esos tipos son los que devuelve el servidor con
    /// `pg_get_function_identity_arguments`.
    Function {
        schema: String,
        name: String,
        #[serde(default)]
        arguments: String,
    },
    Procedure {
        schema: String,
        name: String,
        #[serde(default)]
        arguments: String,
    },
    /// Un trigger se nombra por su tabla: `COMMENT ON TRIGGER x ON tabla`.
    Trigger {
        schema: String,
        table: String,
        name: String,
    },
    Constraint {
        schema: String,
        table: String,
        name: String,
    },
    Policy {
        schema: String,
        table: String,
        name: String,
    },
}

/// Un cambio de comentario pendiente.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentChange {
    pub target: CommentTarget,
    /// `None` —o en blanco— borra el comentario.
    #[serde(default)]
    pub comment: Option<String>,
}

/// Cómo se escribe el nombre del objeto y con qué palabra clave.
fn target_sql(target: &CommentTarget) -> Result<String> {
    let sql = match target {
        CommentTarget::Table { schema, name } => {
            format!("TABLE {}", qualified(schema, require(name)?))
        }
        CommentTarget::Column {
            schema,
            table,
            column,
        } => format!(
            "COLUMN {}.{}",
            qualified(schema, require(table)?),
            quote_ident(require(column)?)
        ),
        CommentTarget::View { schema, name } => {
            format!("VIEW {}", qualified(schema, require(name)?))
        }
        CommentTarget::MaterializedView { schema, name } => {
            format!("MATERIALIZED VIEW {}", qualified(schema, require(name)?))
        }
        CommentTarget::ForeignTable { schema, name } => {
            format!("FOREIGN TABLE {}", qualified(schema, require(name)?))
        }
        CommentTarget::Sequence { schema, name } => {
            format!("SEQUENCE {}", qualified(schema, require(name)?))
        }
        CommentTarget::Index { schema, name } => {
            format!("INDEX {}", qualified(schema, require(name)?))
        }
        CommentTarget::Type { schema, name } => {
            format!("TYPE {}", qualified(schema, require(name)?))
        }
        CommentTarget::Domain { schema, name } => {
            format!("DOMAIN {}", qualified(schema, require(name)?))
        }
        CommentTarget::Schema { name } => format!("SCHEMA {}", quote_ident(require(name)?)),
        CommentTarget::Database { name } => format!("DATABASE {}", quote_ident(require(name)?)),
        CommentTarget::Role { name } => format!("ROLE {}", role_name(require(name)?)),
        CommentTarget::Extension { name } => format!("EXTENSION {}", quote_ident(require(name)?)),
        CommentTarget::Function {
            schema,
            name,
            arguments,
        } => format!(
            "FUNCTION {}({})",
            qualified(schema, require(name)?),
            arguments.trim()
        ),
        CommentTarget::Procedure {
            schema,
            name,
            arguments,
        } => format!(
            "PROCEDURE {}({})",
            qualified(schema, require(name)?),
            arguments.trim()
        ),
        CommentTarget::Trigger {
            schema,
            table,
            name,
        } => format!(
            "TRIGGER {} ON {}",
            quote_ident(require(name)?),
            qualified(schema, require(table)?)
        ),
        CommentTarget::Constraint {
            schema,
            table,
            name,
        } => format!(
            "CONSTRAINT {} ON {}",
            quote_ident(require(name)?),
            qualified(schema, require(table)?)
        ),
        CommentTarget::Policy {
            schema,
            table,
            name,
        } => format!(
            "POLICY {} ON {}",
            quote_ident(require(name)?),
            qualified(schema, require(table)?)
        ),
    };

    Ok(sql)
}

fn require(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Config(
            "falta el nombre del objeto que se comenta".to_owned(),
        ));
    }
    Ok(name)
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(changes: &[CommentChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &CommentChange) -> Result<Statement> {
    let text = match change.comment.as_deref().map(str::trim) {
        Some(text) if !text.is_empty() => format!("'{}'", text.replace('\'', "''")),
        // En blanco borra: un comentario vacío se ve igual que ninguno pero ocupa lugar en el
        // catálogo y en la interfaz.
        _ => "NULL".to_owned(),
    };

    Ok(Statement {
        sql: format!("COMMENT ON {} IS {text}", target_sql(&change.target)?),
    })
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[CommentChange]) -> Result<()> {
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

    fn sql(target: CommentTarget, comment: Option<&str>) -> String {
        statements(&[CommentChange {
            target,
            comment: comment.map(str::to_owned),
        }])
        .expect("tenía que generar la sentencia")
        .remove(0)
        .sql
    }

    #[test]
    fn comenta_una_tabla_y_una_columna() {
        assert_eq!(
            sql(
                CommentTarget::Table {
                    schema: "public".into(),
                    name: "clientes".into(),
                },
                Some("Clientes activos e históricos")
            ),
            "COMMENT ON TABLE public.clientes IS 'Clientes activos e históricos'"
        );

        assert_eq!(
            sql(
                CommentTarget::Column {
                    schema: "public".into(),
                    table: "clientes".into(),
                    column: "estado".into(),
                },
                Some("activo | inactivo")
            ),
            "COMMENT ON COLUMN public.clientes.estado IS 'activo | inactivo'"
        );
    }

    #[test]
    fn sin_texto_o_en_blanco_borra_el_comentario() {
        assert_eq!(
            sql(
                CommentTarget::Schema {
                    name: "ventas".into(),
                },
                None
            ),
            "COMMENT ON SCHEMA ventas IS NULL"
        );

        assert_eq!(
            sql(
                CommentTarget::Schema {
                    name: "ventas".into(),
                },
                Some("   ")
            ),
            "COMMENT ON SCHEMA ventas IS NULL"
        );
    }

    #[test]
    fn escapa_la_comilla_del_texto() {
        assert_eq!(
            sql(
                CommentTarget::Database {
                    name: "ventas".into(),
                },
                Some("la base de 'producción'")
            ),
            "COMMENT ON DATABASE ventas IS 'la base de ''producción'''"
        );
    }

    #[test]
    fn nombra_los_objetos_que_cuelgan_de_una_tabla() {
        assert_eq!(
            sql(
                CommentTarget::Trigger {
                    schema: "public".into(),
                    table: "clientes".into(),
                    name: "audita".into(),
                },
                Some("bitácora")
            ),
            "COMMENT ON TRIGGER audita ON public.clientes IS 'bitácora'"
        );

        assert_eq!(
            sql(
                CommentTarget::Policy {
                    schema: "public".into(),
                    table: "clientes".into(),
                    name: "solo_propios".into(),
                },
                None
            ),
            "COMMENT ON POLICY solo_propios ON public.clientes IS NULL"
        );
    }

    #[test]
    fn una_funcion_lleva_su_firma() {
        assert_eq!(
            sql(
                CommentTarget::Function {
                    schema: "public".into(),
                    name: "total".into(),
                    arguments: "integer, text".into(),
                },
                Some("suma")
            ),
            "COMMENT ON FUNCTION public.total(integer, text) IS 'suma'"
        );

        // Sin argumentos quedan los paréntesis vacíos, que es lo que espera el servidor.
        assert_eq!(
            sql(
                CommentTarget::Function {
                    schema: "public".into(),
                    name: "ahora".into(),
                    arguments: String::new(),
                },
                None
            ),
            "COMMENT ON FUNCTION public.ahora() IS NULL"
        );
    }

    #[test]
    fn public_no_se_cita_como_rol() {
        assert_eq!(
            sql(
                CommentTarget::Role {
                    name: "public".into(),
                },
                Some("todos")
            ),
            "COMMENT ON ROLE PUBLIC IS 'todos'"
        );
    }

    #[test]
    fn un_objeto_sin_nombre_no_se_genera() {
        assert!(statements(&[CommentChange {
            target: CommentTarget::Table {
                schema: "public".into(),
                name: "  ".into(),
            },
            comment: Some("x".into()),
        }])
        .is_err());
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        assert_eq!(
            sql(
                CommentTarget::MaterializedView {
                    schema: "mi esquema".into(),
                    name: "Resumen".into(),
                },
                Some("mensual")
            ),
            "COMMENT ON MATERIALIZED VIEW \"mi esquema\".\"Resumen\" IS 'mensual'"
        );
    }
}
