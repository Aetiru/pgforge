//! Altas, cambios y bajas desde la grilla.
//!
//! Tres reglas sostienen todo lo de acá:
//!
//! 1. **Una fila es su clave.** Sin clave no se genera nada; la grilla ya se abrió en solo lectura.
//! 2. **Nadie pisa a nadie.** Cada `UPDATE` lleva, además de la clave, el valor original de las
//!    columnas que cambia. Si afecta cero filas es porque otro las tocó en el medio, y eso se
//!    informa en vez de sobrescribirlo.
//! 3. **Todo o nada.** El lote entero va en una transacción: guardar diez cambios y que queden
//!    cuatro aplicados es peor que no guardar ninguno.
//!
//! [`statements`] es pura a propósito. Además de ser lo único que se puede verificar sin servidor,
//! es lo que alimenta la vista previa: la interfaz muestra exactamente el SQL que se va a ejecutar,
//! no una reconstrucción parecida.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;

use crate::conn::ServerHandle;
use crate::ddl::quote_ident;
use crate::error::{Error, Result};

use super::shape::TableShape;

/// Valores de una fila, por nombre de columna. Ordenado para que el SQL generado sea estable y se
/// pueda comparar en un test.
pub type Values = BTreeMap<String, Option<String>>;

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    Insert {
        values: Values,
    },
    #[serde(rename_all = "camelCase")]
    Update {
        key: Vec<String>,
        /// Lo que la grilla leyó del servidor para las columnas que se cambian.
        original: Values,
        changes: Values,
    },
    Delete {
        key: Vec<String>,
    },
}

/// Una sentencia lista para ejecutar, con sus parámetros por separado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statement {
    pub sql: String,
    pub params: Vec<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub inserted: usize,
    pub updated: usize,
    pub deleted: usize,
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(shape: &TableShape, changes: &[Change]) -> Result<Vec<Statement>> {
    if let Some(reason) = &shape.read_only {
        return Err(Error::Config(format!("la tabla no es editable: {reason}")));
    }

    changes
        .iter()
        .map(|change| match change {
            Change::Insert { values } => insert(shape, values),
            Change::Update {
                key,
                original,
                changes,
            } => update(shape, key, original, changes),
            Change::Delete { key } => delete(shape, key),
        })
        .collect()
}

fn insert(shape: &TableShape, values: &Values) -> Result<Statement> {
    let table = table_name(shape);

    // Las columnas generadas y las de identidad las calcula el servidor: mandarlas es un error, no
    // un valor por omisión.
    let writable: Vec<(&String, &Option<String>)> = values
        .iter()
        .filter(|(name, _)| !shape.column(name).is_some_and(|column| column.generated))
        .collect();

    for (name, _) in &writable {
        if shape.column(name).is_none() {
            return Err(Error::Config(format!(
                "{table} no tiene ninguna columna llamada {name}"
            )));
        }
    }

    // Una fila entera por omisión es legítima: una tabla de una sola columna de identidad se llena
    // así, y sin esto habría que inventarle un valor.
    if writable.is_empty() {
        return Ok(Statement {
            sql: format!("INSERT INTO {table} DEFAULT VALUES"),
            params: Vec::new(),
        });
    }

    let mut params = Vec::with_capacity(writable.len());
    let mut columns = Vec::with_capacity(writable.len());
    let mut placeholders = Vec::with_capacity(writable.len());

    for (name, value) in writable {
        columns.push(quote_ident(name));
        placeholders.push(cast_param(params.len() + 1, shape, name));
        params.push(value.clone());
    }

    Ok(Statement {
        sql: format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        ),
        params,
    })
}

fn update(
    shape: &TableShape,
    key: &[String],
    original: &Values,
    changes: &Values,
) -> Result<Statement> {
    let table = table_name(shape);

    if changes.is_empty() {
        return Err(Error::Config(
            "no hay ninguna columna que cambiar en esta fila".to_owned(),
        ));
    }

    for name in changes.keys() {
        match shape.column(name) {
            None => {
                return Err(Error::Config(format!(
                    "{table} no tiene ninguna columna llamada {name}"
                )))
            }
            Some(column) if column.generated => {
                return Err(Error::Config(format!(
                    "{name} la calcula el servidor y no se puede escribir"
                )))
            }
            Some(_) => {}
        }
    }

    let mut params: Vec<Option<String>> = Vec::new();

    let assignments: Vec<String> = changes
        .iter()
        .map(|(name, value)| {
            let assignment = format!(
                "{} = {}",
                quote_ident(name),
                cast_param(params.len() + 1, shape, name)
            );
            params.push(value.clone());
            assignment
        })
        .collect();

    let mut conditions = key_conditions(shape, key, &mut params)?;

    // La guarda de concurrencia. `IS NOT DISTINCT FROM` y no `=` porque comparar un NULL con `=` no
    // da ni cierto ni falso, y la condición no se cumpliría nunca sobre una columna vacía.
    for name in changes.keys() {
        let value = original.get(name).cloned().flatten();
        conditions.push(format!(
            "{} IS NOT DISTINCT FROM {}",
            quote_ident(name),
            cast_param(params.len() + 1, shape, name)
        ));
        params.push(value);
    }

    Ok(Statement {
        sql: format!(
            "UPDATE {table} SET {} WHERE {}",
            assignments.join(", "),
            conditions.join(" AND ")
        ),
        params,
    })
}

fn delete(shape: &TableShape, key: &[String]) -> Result<Statement> {
    let mut params = Vec::new();
    let conditions = key_conditions(shape, key, &mut params)?;

    Ok(Statement {
        sql: format!(
            "DELETE FROM {} WHERE {}",
            table_name(shape),
            conditions.join(" AND ")
        ),
        params,
    })
}

/// Las condiciones que señalan una fila, agregando sus valores a `params`.
///
/// Van con `=` y no con `IS NOT DISTINCT FROM` porque las columnas de una clave son `NOT NULL` por
/// construcción, y la igualdad es la única forma que aprovecha el índice.
fn key_conditions(
    shape: &TableShape,
    key: &[String],
    params: &mut Vec<Option<String>>,
) -> Result<Vec<String>> {
    let columns = shape.key_columns();

    if columns.is_empty() {
        return Err(Error::Config(
            "la tabla no tiene clave con la que identificar la fila".to_owned(),
        ));
    }
    if key.len() != columns.len() {
        return Err(Error::Config(format!(
            "la fila trae {} valores de clave y la clave tiene {}",
            key.len(),
            columns.len()
        )));
    }

    Ok(columns
        .iter()
        .zip(key)
        .map(|(name, value)| {
            let condition = format!(
                "{} = {}",
                quote_ident(name),
                cast_param(params.len() + 1, shape, name)
            );
            params.push(Some(value.clone()));
            condition
        })
        .collect())
}

fn table_name(shape: &TableShape) -> String {
    format!(
        "{}.{}",
        quote_ident(&shape.schema),
        quote_ident(&shape.name)
    )
}

/// `$n::text::tipo`. El mismo doble casteo que usa la paginación, y por el mismo motivo: pasando
/// por `text` es el servidor el que convierte, con la función de entrada del tipo, así que sirve
/// para cualquier tipo que exista.
fn cast_param(position: usize, shape: &TableShape, column: &str) -> String {
    match shape.column(column) {
        Some(column) => format!("${position}::text::{}", column.type_name),
        None => format!("${position}::text"),
    }
}

/// Aplica los cambios en una sola transacción.
pub async fn apply(
    handle: &ServerHandle,
    database: &str,
    shape: &TableShape,
    changes: &[Change],
) -> Result<Applied> {
    let statements = statements(shape, changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    let mut applied = Applied {
        inserted: 0,
        updated: 0,
        deleted: 0,
    };

    for (statement, change) in statements.iter().zip(changes) {
        let params: Vec<&(dyn ToSql + Sync)> = statement
            .params
            .iter()
            .map(|value| value as &(dyn ToSql + Sync))
            .collect();

        let affected = transaction.execute(statement.sql.as_str(), &params).await?;

        if affected != 1 {
            // Al soltar la transacción sin confirmar, el servidor revierte todo el lote.
            return Err(conflict(change, affected));
        }

        match change {
            Change::Insert { .. } => applied.inserted += 1,
            Change::Update { .. } => applied.updated += 1,
            Change::Delete { .. } => applied.deleted += 1,
        }
    }

    transaction.commit().await?;
    Ok(applied)
}

fn conflict(change: &Change, affected: u64) -> Error {
    match (change, affected) {
        (Change::Update { .. }, 0) => Error::Conflict(
            "la fila cambió en el servidor desde que la leíste: alguien más la modificó o la \
             borró. No se aplicó nada; refrescá para ver cómo quedó."
                .to_owned(),
        ),
        (Change::Delete { .. }, 0) => Error::Conflict(
            "la fila ya no está: alguien más la borró. No se aplicó nada.".to_owned(),
        ),
        // Más de una fila afectada significa que la clave no identifica lo que decía identificar.
        // Es lo bastante grave como para revertir todo el lote sin intentar adivinar.
        (_, n) => Error::Conflict(format!(
            "una sentencia afectó {n} filas en lugar de una. No se aplicó nada."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::shape::{Column, Key, KeyKind};

    fn column(name: &str, type_name: &str, generated: bool) -> Column {
        Column {
            name: name.into(),
            type_name: type_name.into(),
            not_null: false,
            default: None,
            generated,
            comment: None,
        }
    }

    fn shape() -> TableShape {
        TableShape {
            oid: 1,
            schema: "public".into(),
            name: "clientes".into(),
            columns: vec![
                column("id", "bigint", true),
                column("nombre", "text", false),
                column("estado", "public.estado", false),
                column("total", "numeric(12,2)", false),
            ],
            key: Some(Key {
                name: "clientes_pkey".into(),
                kind: KeyKind::Primary,
                columns: vec!["id".into()],
            }),
            read_only: None,
        }
    }

    fn values(pairs: &[(&str, Option<&str>)]) -> Values {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.map(str::to_owned)))
            .collect()
    }

    fn one(shape: &TableShape, change: Change) -> Statement {
        statements(shape, &[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn el_update_lleva_la_clave_y_el_valor_original() {
        let statement = one(
            &shape(),
            Change::Update {
                key: vec!["7".into()],
                original: values(&[("nombre", Some("Ana"))]),
                changes: values(&[("nombre", Some("Ana María"))]),
            },
        );

        assert_eq!(
            statement.sql,
            "UPDATE public.clientes SET nombre = $1::text::text \
             WHERE id = $2::text::bigint AND nombre IS NOT DISTINCT FROM $3::text::text"
        );
        assert_eq!(
            statement.params,
            vec![
                Some("Ana María".to_owned()),
                Some("7".to_owned()),
                Some("Ana".to_owned())
            ],
            "el orden de los parámetros tiene que seguir al de los marcadores"
        );
    }

    #[test]
    fn una_columna_vacia_se_compara_con_is_not_distinct_from() {
        let statement = one(
            &shape(),
            Change::Update {
                key: vec!["7".into()],
                original: values(&[("nombre", None)]),
                changes: values(&[("nombre", Some("Ana"))]),
            },
        );

        assert!(
            statement.sql.contains("nombre IS NOT DISTINCT FROM"),
            "con `=` la condición no se cumpliría nunca sobre un NULL: {}",
            statement.sql
        );
        assert_eq!(statement.params[2], None);
    }

    #[test]
    fn el_insert_omite_las_columnas_generadas() {
        let statement = one(
            &shape(),
            Change::Insert {
                values: values(&[("id", Some("99")), ("nombre", Some("Ana"))]),
            },
        );

        assert_eq!(
            statement.sql, "INSERT INTO public.clientes (nombre) VALUES ($1::text::text)",
            "la identidad la calcula el servidor: mandarla es un error, no un valor"
        );
        assert_eq!(statement.params, vec![Some("Ana".to_owned())]);
    }

    #[test]
    fn una_fila_entera_por_omision_es_valida() {
        let statement = one(
            &shape(),
            Change::Insert {
                values: Values::new(),
            },
        );

        assert_eq!(statement.sql, "INSERT INTO public.clientes DEFAULT VALUES");
        assert!(statement.params.is_empty());
    }

    #[test]
    fn el_delete_usa_la_clave() {
        let statement = one(
            &shape(),
            Change::Delete {
                key: vec!["7".into()],
            },
        );

        assert_eq!(
            statement.sql,
            "DELETE FROM public.clientes WHERE id = $1::text::bigint"
        );
        assert_eq!(statement.params, vec![Some("7".to_owned())]);
    }

    #[test]
    fn el_tipo_de_cada_columna_llega_al_casteo() {
        let statement = one(
            &shape(),
            Change::Update {
                key: vec!["7".into()],
                original: values(&[("estado", None), ("total", None)]),
                changes: values(&[("estado", Some("activo")), ("total", Some("12.50"))]),
            },
        );

        assert!(
            statement.sql.contains("::text::public.estado"),
            "{}",
            statement.sql
        );
        assert!(
            statement.sql.contains("::text::numeric(12,2)"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn la_clave_compuesta_va_entera_en_el_where() {
        let mut shape = shape();
        shape.key = Some(Key {
            name: "pk".into(),
            kind: KeyKind::Primary,
            columns: vec!["id".into(), "nombre".into()],
        });

        let statement = one(
            &shape,
            Change::Delete {
                key: vec!["7".into(), "Ana".into()],
            },
        );
        assert_eq!(
            statement.sql,
            "DELETE FROM public.clientes \
             WHERE id = $1::text::bigint AND nombre = $2::text::text"
        );
    }

    #[test]
    fn una_tabla_sin_clave_no_genera_nada() {
        let mut shape = shape();
        shape.key = None;
        shape.read_only = Some("sin clave".into());

        let error = statements(
            &shape,
            &[Change::Delete {
                key: vec!["7".into()],
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("no es editable"), "{error}");
    }

    #[test]
    fn una_clave_incompleta_no_genera_nada() {
        let mut shape = shape();
        shape.key = Some(Key {
            name: "pk".into(),
            kind: KeyKind::Primary,
            columns: vec!["id".into(), "nombre".into()],
        });

        // Con un solo valor, el WHERE señalaría muchas más filas de las que el usuario cree.
        assert!(statements(
            &shape,
            &[Change::Delete {
                key: vec!["7".into()]
            }]
        )
        .is_err());
    }

    #[test]
    fn no_se_escribe_una_columna_generada() {
        let error = statements(
            &shape(),
            &[Change::Update {
                key: vec!["7".into()],
                original: values(&[("id", Some("7"))]),
                changes: values(&[("id", Some("8"))]),
            }],
        )
        .unwrap_err();
        assert!(error.to_string().contains("calcula el servidor"), "{error}");
    }

    #[test]
    fn una_columna_que_no_existe_no_genera_nada() {
        assert!(statements(
            &shape(),
            &[Change::Update {
                key: vec!["7".into()],
                original: Values::new(),
                changes: values(&[("inventada", Some("x"))]),
            }]
        )
        .is_err());
    }

    #[test]
    fn un_update_sin_cambios_no_genera_nada() {
        assert!(statements(
            &shape(),
            &[Change::Update {
                key: vec!["7".into()],
                original: Values::new(),
                changes: Values::new(),
            }]
        )
        .is_err());
    }

    #[test]
    fn el_lote_conserva_el_orden_en_que_se_editó() {
        let generadas = statements(
            &shape(),
            &[
                Change::Insert {
                    values: values(&[("nombre", Some("Ana"))]),
                },
                Change::Delete {
                    key: vec!["7".into()],
                },
            ],
        )
        .unwrap();

        assert_eq!(generadas.len(), 2);
        assert!(generadas[0].sql.starts_with("INSERT"));
        assert!(generadas[1].sql.starts_with("DELETE"));
    }
}
