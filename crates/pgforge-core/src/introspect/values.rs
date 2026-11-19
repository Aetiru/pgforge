//! Valores probables de una columna, para completar `estado = '…'` mientras se escribe.
//!
//! Dos fuentes, ninguna recorre la tabla: las etiquetas de un `enum` salen de `pg_enum`, y para el
//! resto se usan los valores más frecuentes que el planificador ya calculó (`pg_stats`). Un
//! `SELECT DISTINCT` sobre una tabla de millones de filas por cada pausa al teclear no es una opción.
//!
//! La consecuencia es que sin `ANALYZE` —o sin permiso de lectura sobre la columna— **no hay
//! sugerencias**, y eso no es un error: es `ColumnValues::none()`.

use serde::Serialize;
use tokio_postgres::Client;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ValuesKind {
    /// Etiquetas de un `enum`, en su orden declarado.
    Enum,
    /// Valores más frecuentes según las estadísticas, de mayor a menor frecuencia.
    Frequent,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValue {
    pub value: String,
    /// Fracción de las filas con ese valor (0 a 1). Solo en `Frequent`.
    pub frequency: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnValues {
    pub kind: ValuesKind,
    pub values: Vec<ColumnValue>,
}

impl ColumnValues {
    pub fn none() -> Self {
        Self {
            kind: ValuesKind::None,
            values: Vec::new(),
        }
    }
}

/// Une cada valor con su frecuencia y corta en `limit`. Las dos listas vienen paralelas del
/// catálogo; si la de frecuencias es más corta (no debería), los que sobran quedan sin ella.
pub fn pair_values(
    values: Vec<String>,
    frequencies: Option<Vec<f32>>,
    limit: usize,
) -> Vec<ColumnValue> {
    let frequencies = frequencies.unwrap_or_default();
    values
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(index, value)| ColumnValue {
            value,
            frequency: frequencies.get(index).map(|f| f64::from(*f)),
        })
        .collect()
}

/// Esquema, tabla y columna viajan como parámetros y nunca se interpolan: llegan del editor.
pub async fn column_values(
    client: &Client,
    schema: &str,
    table: &str,
    column: &str,
    limit: usize,
) -> Result<ColumnValues> {
    let limit = limit.clamp(1, 200);

    let ty = client
        .query_opt(
            "SELECT t.oid, t.typtype::text
               FROM pg_catalog.pg_attribute a
               JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
               JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
              WHERE n.nspname = $1 AND c.relname = $2 AND a.attname = $3
                AND a.attnum > 0 AND NOT a.attisdropped",
            &[&schema, &table, &column],
        )
        .await?;
    // Sin la columna no hay nada que sugerir; un nombre mal escrito no es un error de la base.
    let Some(ty) = ty else {
        return Ok(ColumnValues::none());
    };

    let type_oid: u32 = ty.get(0);
    let kind: String = ty.get(1);
    if kind == "e" {
        let labels: Vec<String> = client
            .query(
                "SELECT enumlabel::text FROM pg_catalog.pg_enum
                  WHERE enumtypid = $1 ORDER BY enumsortorder",
                &[&type_oid],
            )
            .await?
            .iter()
            .map(|row| row.get(0))
            .collect();
        return Ok(ColumnValues {
            kind: ValuesKind::Enum,
            values: pair_values(labels, None, limit),
        });
    }

    // `inherited = false` primero: en una tabla particionada o con herencia hay dos filas, la de la
    // tabla sola y la del árbol entero, y cualquiera de las dos sirve para sugerir.
    let stats = client
        .query_opt(
            "SELECT most_common_vals::text::text[], most_common_freqs
               FROM pg_catalog.pg_stats
              WHERE schemaname = $1 AND tablename = $2 AND attname = $3
              ORDER BY inherited
              LIMIT 1",
            &[&schema, &table, &column],
        )
        .await?;
    let Some(stats) = stats else {
        return Ok(ColumnValues::none());
    };
    let common: Option<Vec<String>> = stats.get(0);
    let frequencies: Option<Vec<f32>> = stats.get(1);
    match common {
        Some(values) if !values.is_empty() => Ok(ColumnValues {
            kind: ValuesKind::Frequent,
            values: pair_values(values, frequencies, limit),
        }),
        _ => Ok(ColumnValues::none()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_cada_valor_con_su_frecuencia() {
        let out = pair_values(
            vec!["activo".into(), "baja".into()],
            Some(vec![0.75, 0.25]),
            10,
        );
        assert_eq!(out[0].value, "activo");
        assert_eq!(out[0].frequency, Some(0.75));
        assert_eq!(out[1].frequency, Some(0.25));
    }

    #[test]
    fn corta_en_el_limite_y_tolera_frecuencias_ausentes_o_cortas() {
        let out = pair_values(vec!["a".into(), "b".into(), "c".into()], None, 2);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|v| v.frequency.is_none()));

        let out = pair_values(vec!["a".into(), "b".into()], Some(vec![0.5]), 10);
        assert_eq!(out[1].frequency, None);
    }

    #[test]
    fn viaja_en_camel_case() {
        let json = serde_json::to_value(ColumnValues {
            kind: ValuesKind::Frequent,
            values: pair_values(vec!["x".into()], Some(vec![0.5]), 5),
        })
        .unwrap();
        assert_eq!(json["kind"], "frequent");
        assert_eq!(json["values"][0]["frequency"], 0.5);
        assert_eq!(
            serde_json::to_value(ColumnValues::none()).unwrap()["kind"],
            "none"
        );
    }
}
