//! Crear, cambiar y borrar tipos definidos por el usuario: enumeraciones y compuestos.
//!
//! El módulo se llama `types` y no `type` porque `type` es palabra reservada de Rust y el nombre de
//! archivo tendría que escribirse `r#type` en cada `use`.
//!
//! Los dominios son también `pg_type`, pero su sintaxis no se parece —restricciones con nombre,
//! `DEFAULT`, `NOT NULL`— así que viven en [`crate::ddl::domain`].
//!
//! Sobre `ALTER TYPE … ADD VALUE` dentro de una transacción: hasta PostgreSQL 11 estaba prohibido y
//! había que mandarlo suelto. Desde la 12 se permite, y el rango soportado por este proyecto
//! arranca en la 13, así que **no hace falta gatearlo por versión** y todo el módulo comparte el
//! molde transaccional del resto del DDL. Lo que sigue prohibido en todas las versiones es *usar*
//! el valor nuevo en la misma transacción que lo agregó; eso lo rechaza el servidor con un mensaje
//! claro y no hay nada que anticipar de este lado.
//!
//! Los tipos de los campos de un compuesto van **crudos**, misma frontera de confianza que el tipo
//! de una columna: no se pueden parametrizar en DDL y los valida el servidor al ejecutar.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident, role_name};

use serde::{Deserialize, Serialize};

/// Un campo de un tipo compuesto.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    /// Va crudo: lo valida el servidor.
    pub data_type: String,
    /// Intercalado del campo, cuando es de texto. Vacío deja el del tipo.
    #[serde(default)]
    pub collation: Option<String>,
}

/// Dónde entra un valor nuevo de una enumeración. Sin posición va al final.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum EnumPosition {
    Before { value: String },
    After { value: String },
}

/// Un cambio de tipo pendiente.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TypeChange {
    CreateEnum {
        schema: String,
        name: String,
        labels: Vec<String>,
    },
    /// Agrega un valor a una enumeración. No se puede quitar ninguno: PostgreSQL no tiene
    /// `DROP VALUE`, y sacar uno exigiría recrear el tipo y todas las columnas que lo usan.
    AddEnumValue {
        schema: String,
        name: String,
        value: String,
        #[serde(default)]
        position: Option<EnumPosition>,
        /// `IF NOT EXISTS`, para que reaplicar el mismo cambio no falle.
        #[serde(default)]
        if_not_exists: bool,
    },
    RenameEnumValue {
        schema: String,
        name: String,
        from: String,
        to: String,
    },
    CreateComposite {
        schema: String,
        name: String,
        fields: Vec<Field>,
    },
    AddCompositeField {
        schema: String,
        name: String,
        field: Field,
    },
    DropCompositeField {
        schema: String,
        name: String,
        field: String,
        cascade: bool,
    },
    AlterCompositeFieldType {
        schema: String,
        name: String,
        field: String,
        /// Va crudo, como el tipo de una columna.
        data_type: String,
        #[serde(default)]
        collation: Option<String>,
        cascade: bool,
    },
    RenameType {
        schema: String,
        name: String,
        new_name: String,
    },
    SetTypeSchema {
        schema: String,
        name: String,
        new_schema: String,
    },
    SetTypeOwner {
        schema: String,
        name: String,
        owner: String,
    },
    DropType {
        schema: String,
        name: String,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Escribe un literal de texto para el SQL, doblando las comillas simples.
///
/// Las etiquetas de una enumeración no se pueden parametrizar —`CREATE TYPE` no acepta
/// parámetros—, así que se interpolan; a diferencia de un identificador, van entre comillas simples
/// y por eso no sirve `quote_ident`.
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn require_field_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Config("un campo necesita un nombre".to_owned()));
    }
    Ok(name)
}

fn require_data_type<'a>(field: &str, data_type: &'a str) -> Result<&'a str> {
    let data_type = data_type.trim();
    if data_type.is_empty() {
        return Err(Error::Config(format!("el campo {field} necesita un tipo")));
    }
    Ok(data_type)
}

fn collation_sql(collation: Option<&str>) -> String {
    match collation.map(str::trim) {
        Some(collation) if !collation.is_empty() => format!(" COLLATE {}", quote_ident(collation)),
        _ => String::new(),
    }
}

fn field_sql(field: &Field) -> Result<String> {
    let name = require_field_name(&field.name)?;
    let data_type = require_data_type(name, &field.data_type)?;

    Ok(format!(
        "{} {data_type}{}",
        quote_ident(name),
        collation_sql(field.collation.as_deref())
    ))
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(changes: &[TypeChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &TypeChange) -> Result<Statement> {
    match change {
        TypeChange::CreateEnum {
            schema,
            name,
            labels,
        } => {
            require_name(name)?;
            if labels.is_empty() {
                return Err(Error::Config(
                    "una enumeración necesita al menos un valor".to_owned(),
                ));
            }
            let labels = labels
                .iter()
                .map(|label| format!("    {}", literal(label)))
                .collect::<Vec<_>>()
                .join(",\n");
            Ok(statement(format!(
                "CREATE TYPE {} AS ENUM (\n{labels}\n)",
                qualified(schema, name)
            )))
        }
        TypeChange::AddEnumValue {
            schema,
            name,
            value,
            position,
            if_not_exists,
        } => {
            if value.is_empty() {
                return Err(Error::Config(
                    "el valor nuevo no puede estar vacío".to_owned(),
                ));
            }
            let position = match position {
                Some(EnumPosition::Before { value }) => format!(" BEFORE {}", literal(value)),
                Some(EnumPosition::After { value }) => format!(" AFTER {}", literal(value)),
                None => String::new(),
            };
            Ok(statement(format!(
                "ALTER TYPE {} ADD VALUE {}{}{position}",
                qualified(schema, name),
                if *if_not_exists { "IF NOT EXISTS " } else { "" },
                literal(value)
            )))
        }
        TypeChange::RenameEnumValue {
            schema,
            name,
            from,
            to,
        } => {
            if to.is_empty() {
                return Err(Error::Config(
                    "el valor nuevo no puede estar vacío".to_owned(),
                ));
            }
            Ok(statement(format!(
                "ALTER TYPE {} RENAME VALUE {} TO {}",
                qualified(schema, name),
                literal(from),
                literal(to)
            )))
        }
        TypeChange::CreateComposite {
            schema,
            name,
            fields,
        } => {
            require_name(name)?;
            if fields.is_empty() {
                return Err(Error::Config(
                    "un tipo compuesto necesita al menos un campo".to_owned(),
                ));
            }
            let fields = fields
                .iter()
                .map(|field| Ok(format!("    {}", field_sql(field)?)))
                .collect::<Result<Vec<_>>>()?
                .join(",\n");
            Ok(statement(format!(
                "CREATE TYPE {} AS (\n{fields}\n)",
                qualified(schema, name)
            )))
        }
        TypeChange::AddCompositeField {
            schema,
            name,
            field,
        } => Ok(statement(format!(
            "ALTER TYPE {} ADD ATTRIBUTE {}",
            qualified(schema, name),
            field_sql(field)?
        ))),
        TypeChange::DropCompositeField {
            schema,
            name,
            field,
            cascade,
        } => Ok(statement(format!(
            "ALTER TYPE {} DROP ATTRIBUTE {}{}",
            qualified(schema, name),
            quote_ident(field),
            if *cascade { " CASCADE" } else { "" }
        ))),
        TypeChange::AlterCompositeFieldType {
            schema,
            name,
            field,
            data_type,
            collation,
            cascade,
        } => {
            let field = require_field_name(field)?;
            let data_type = require_data_type(field, data_type)?;
            Ok(statement(format!(
                "ALTER TYPE {} ALTER ATTRIBUTE {} TYPE {data_type}{}{}",
                qualified(schema, name),
                quote_ident(field),
                collation_sql(collation.as_deref()),
                if *cascade { " CASCADE" } else { "" }
            )))
        }
        TypeChange::RenameType {
            schema,
            name,
            new_name,
        } => {
            require_name(new_name)?;
            Ok(statement(format!(
                "ALTER TYPE {} RENAME TO {}",
                qualified(schema, name),
                quote_ident(new_name)
            )))
        }
        TypeChange::SetTypeSchema {
            schema,
            name,
            new_schema,
        } => {
            require_name(new_schema)?;
            Ok(statement(format!(
                "ALTER TYPE {} SET SCHEMA {}",
                qualified(schema, name),
                quote_ident(new_schema)
            )))
        }
        TypeChange::SetTypeOwner {
            schema,
            name,
            owner,
        } => {
            require_name(owner)?;
            Ok(statement(format!(
                "ALTER TYPE {} OWNER TO {}",
                qualified(schema, name),
                role_name(owner)
            )))
        }
        TypeChange::DropType {
            schema,
            name,
            cascade,
        } => Ok(statement(format!(
            "DROP TYPE {}{}",
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
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[TypeChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Qué clase de tipo es, para que la interfaz sepa qué formulario abrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TypeKind {
    Enum,
    Composite,
    Domain,
    /// Base, rango, pseudotipo: se pueden leer y borrar, pero este módulo no los edita.
    Other,
}

/// Lo que hay que mostrar de un tipo al abrir «Editar».
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeInfo {
    pub schema: String,
    pub name: String,
    pub owner: String,
    pub kind: TypeKind,
    /// Los valores, en orden, cuando es una enumeración.
    pub labels: Vec<String>,
    /// Los campos, en orden, cuando es un compuesto.
    pub fields: Vec<Field>,
    pub comment: Option<String>,
}

/// Lee la definición de un tipo.
pub async fn info(handle: &ServerHandle, database: &str, oid: u32) -> Result<TypeInfo> {
    let client = handle.client(database).await?;

    let row = client
        .query_one(
            "SELECT n.nspname::text,
                    t.typname::text,
                    pg_catalog.pg_get_userbyid(t.typowner)::text,
                    t.typtype::text,
                    pg_catalog.obj_description(t.oid, 'pg_type')
               FROM pg_catalog.pg_type t
               JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
              WHERE t.oid = $1",
            &[&oid],
        )
        .await?;

    let typtype: String = row.get(3);
    let kind = match typtype.as_str() {
        "e" => TypeKind::Enum,
        "c" => TypeKind::Composite,
        "d" => TypeKind::Domain,
        _ => TypeKind::Other,
    };

    let labels = if kind == TypeKind::Enum {
        client
            .query(
                "SELECT e.enumlabel::text
                   FROM pg_catalog.pg_enum e
                  WHERE e.enumtypid = $1
                  ORDER BY e.enumsortorder",
                &[&oid],
            )
            .await?
            .into_iter()
            .map(|row| row.get(0))
            .collect()
    } else {
        Vec::new()
    };

    let fields = if kind == TypeKind::Composite {
        client
            .query(
                "SELECT a.attname::text,
                        pg_catalog.format_type(a.atttypid, a.atttypmod),
                        co.collname::text
                   FROM pg_catalog.pg_attribute a
                   JOIN pg_catalog.pg_type t ON t.typrelid = a.attrelid
              LEFT JOIN pg_catalog.pg_collation co ON co.oid = a.attcollation
                  WHERE t.oid = $1 AND a.attnum > 0 AND NOT a.attisdropped
                  ORDER BY a.attnum",
                &[&oid],
            )
            .await?
            .into_iter()
            .map(|row| Field {
                name: row.get(0),
                data_type: row.get(1),
                collation: row.get(2),
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(TypeInfo {
        schema: row.get(0),
        name: row.get(1),
        owner: row.get(2),
        kind,
        labels,
        fields,
        comment: row.get(4),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: TypeChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    fn field(name: &str, data_type: &str) -> Field {
        Field {
            name: name.into(),
            data_type: data_type.into(),
            collation: None,
        }
    }

    #[test]
    fn crea_una_enumeracion() {
        let statement = one_statement(TypeChange::CreateEnum {
            schema: "public".into(),
            name: "estado".into(),
            labels: vec!["activo".into(), "inactivo".into()],
        });
        assert_eq!(
            statement.sql,
            "CREATE TYPE public.estado AS ENUM (\n    'activo',\n    'inactivo'\n)"
        );
    }

    #[test]
    fn una_enumeracion_sin_valores_no_se_genera() {
        assert!(statements(&[TypeChange::CreateEnum {
            schema: "public".into(),
            name: "estado".into(),
            labels: vec![],
        }])
        .is_err());
    }

    #[test]
    fn escapa_la_comilla_de_una_etiqueta() {
        let statement = one_statement(TypeChange::CreateEnum {
            schema: "public".into(),
            name: "estado".into(),
            labels: vec!["en tránsito 'urgente'".into()],
        });
        assert!(
            statement.sql.contains("'en tránsito ''urgente'''"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn agrega_un_valor_al_final_y_en_una_posicion() {
        let statement = one_statement(TypeChange::AddEnumValue {
            schema: "public".into(),
            name: "estado".into(),
            value: "pausado".into(),
            position: None,
            if_not_exists: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.estado ADD VALUE 'pausado'"
        );

        let statement = one_statement(TypeChange::AddEnumValue {
            schema: "public".into(),
            name: "estado".into(),
            value: "pausado".into(),
            position: Some(EnumPosition::Before {
                value: "inactivo".into(),
            }),
            if_not_exists: true,
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.estado ADD VALUE IF NOT EXISTS 'pausado' BEFORE 'inactivo'"
        );
    }

    #[test]
    fn renombra_un_valor() {
        let statement = one_statement(TypeChange::RenameEnumValue {
            schema: "public".into(),
            name: "estado".into(),
            from: "activo".into(),
            to: "vigente".into(),
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.estado RENAME VALUE 'activo' TO 'vigente'"
        );
    }

    #[test]
    fn crea_un_compuesto() {
        let statement = one_statement(TypeChange::CreateComposite {
            schema: "public".into(),
            name: "direccion".into(),
            fields: vec![field("calle", "text"), field("numero", "integer")],
        });
        assert_eq!(
            statement.sql,
            "CREATE TYPE public.direccion AS (\n    calle text,\n    numero integer\n)"
        );
    }

    #[test]
    fn un_campo_sin_tipo_no_se_genera() {
        assert!(statements(&[TypeChange::CreateComposite {
            schema: "public".into(),
            name: "direccion".into(),
            fields: vec![field("calle", "  ")],
        }])
        .is_err());
    }

    #[test]
    fn agrega_quita_y_cambia_un_campo() {
        let statement = one_statement(TypeChange::AddCompositeField {
            schema: "public".into(),
            name: "direccion".into(),
            field: field("piso", "text"),
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.direccion ADD ATTRIBUTE piso text"
        );

        let statement = one_statement(TypeChange::DropCompositeField {
            schema: "public".into(),
            name: "direccion".into(),
            field: "piso".into(),
            cascade: true,
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.direccion DROP ATTRIBUTE piso CASCADE"
        );

        let statement = one_statement(TypeChange::AlterCompositeFieldType {
            schema: "public".into(),
            name: "direccion".into(),
            field: "numero".into(),
            data_type: "bigint".into(),
            collation: None,
            cascade: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.direccion ALTER ATTRIBUTE numero TYPE bigint"
        );
    }

    #[test]
    fn un_campo_lleva_su_intercalado() {
        let statement = one_statement(TypeChange::AddCompositeField {
            schema: "public".into(),
            name: "direccion".into(),
            field: Field {
                name: "calle".into(),
                data_type: "text".into(),
                collation: Some("es_AR".into()),
            },
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.direccion ADD ATTRIBUTE calle text COLLATE \"es_AR\""
        );
    }

    #[test]
    fn renombra_mueve_y_cambia_de_dueno() {
        let statement = one_statement(TypeChange::RenameType {
            schema: "public".into(),
            name: "estado".into(),
            new_name: "estado_viejo".into(),
        });
        assert_eq!(
            statement.sql,
            "ALTER TYPE public.estado RENAME TO estado_viejo"
        );

        let statement = one_statement(TypeChange::SetTypeSchema {
            schema: "public".into(),
            name: "estado".into(),
            new_schema: "archivo".into(),
        });
        assert_eq!(statement.sql, "ALTER TYPE public.estado SET SCHEMA archivo");

        let statement = one_statement(TypeChange::SetTypeOwner {
            schema: "public".into(),
            name: "estado".into(),
            owner: "ventas".into(),
        });
        assert_eq!(statement.sql, "ALTER TYPE public.estado OWNER TO ventas");
    }

    #[test]
    fn borra_con_y_sin_cascade() {
        let statement = one_statement(TypeChange::DropType {
            schema: "public".into(),
            name: "estado".into(),
            cascade: false,
        });
        assert_eq!(statement.sql, "DROP TYPE public.estado");

        let statement = one_statement(TypeChange::DropType {
            schema: "public".into(),
            name: "estado".into(),
            cascade: true,
        });
        assert_eq!(statement.sql, "DROP TYPE public.estado CASCADE");
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = one_statement(TypeChange::CreateComposite {
            schema: "mi esquema".into(),
            name: "Direccion".into(),
            fields: vec![field("calle principal", "text")],
        });
        assert!(
            statement
                .sql
                .starts_with("CREATE TYPE \"mi esquema\".\"Direccion\" AS (\n    \"calle principal\" text"),
            "{}",
            statement.sql
        );
    }
}
