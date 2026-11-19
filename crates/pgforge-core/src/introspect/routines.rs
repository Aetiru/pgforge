//! Firmas de funciones y procedimientos, para la ayuda de parámetros del editor.
//!
//! Se piden **por nombre y a pedido** —al abrir el paréntesis de una llamada— y no se trae el
//! catálogo entero: una base con extensiones tiene miles de funciones y la ayuda solo necesita las
//! sobrecargas de la que se está escribiendo. El texto de los argumentos y del retorno lo escribe el
//! servidor (`pg_get_function_arguments`, `pg_get_function_result`), igual que el DDL: reconstruirlo
//! desde `pg_proc` sería inventar la mitad (valores por omisión, `VARIADIC`, `OUT`).

use serde::Serialize;
use tokio_postgres::Client;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutineKind {
    Function,
    Procedure,
    Aggregate,
    Window,
}

impl RoutineKind {
    /// `prokind` existe desde PG 11, así que sirve en todo el rango soportado y no hace falta un
    /// predicado de `ServerCaps`. Un valor desconocido se trata como función.
    pub fn from_prokind(code: &str) -> Self {
        match code {
            "p" => RoutineKind::Procedure,
            "a" => RoutineKind::Aggregate,
            "w" => RoutineKind::Window,
            _ => RoutineKind::Function,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineSignature {
    pub schema: String,
    pub name: String,
    /// Como los escribiría `CREATE FUNCTION`: `field text, source timestamp with time zone`.
    pub arguments: String,
    /// `None` en un procedimiento, que no devuelve nada.
    pub result: Option<String>,
    pub comment: Option<String>,
    pub kind: RoutineKind,
}

/// Cuántas sobrecargas se traen como máximo: la ayuda las muestra en una lista corta.
const LIMIT: i64 = 20;

/// `schema` en `Some` restringe a ese esquema (`pg_catalog.now(`); en `None` se buscan todos, que es
/// lo que se puede hacer sin conocer el `search_path` de la pestaña.
pub async fn signatures(
    client: &Client,
    schema: Option<&str>,
    name: &str,
) -> Result<Vec<RoutineSignature>> {
    let rows = client
        .query(
            "SELECT n.nspname::text,
                    p.proname::text,
                    pg_catalog.pg_get_function_arguments(p.oid),
                    pg_catalog.pg_get_function_result(p.oid),
                    pg_catalog.obj_description(p.oid, 'pg_proc'),
                    p.prokind::text
               FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
              WHERE p.proname = $1
                AND ($2::text IS NULL OR n.nspname = $2::text)
              ORDER BY n.nspname, p.pronargs, p.oid
              LIMIT $3",
            &[&name, &schema, &LIMIT],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|row| RoutineSignature {
            schema: row.get(0),
            name: row.get(1),
            arguments: row.get(2),
            result: row.get(3),
            comment: row.get(4),
            kind: RoutineKind::from_prokind(&row.get::<_, String>(5)),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduce_prokind() {
        assert_eq!(RoutineKind::from_prokind("f"), RoutineKind::Function);
        assert_eq!(RoutineKind::from_prokind("p"), RoutineKind::Procedure);
        assert_eq!(RoutineKind::from_prokind("a"), RoutineKind::Aggregate);
        assert_eq!(RoutineKind::from_prokind("w"), RoutineKind::Window);
        assert_eq!(RoutineKind::from_prokind("?"), RoutineKind::Function);
    }

    #[test]
    fn viaja_en_camel_case_con_el_tipo_en_minuscula() {
        let json = serde_json::to_value(RoutineSignature {
            schema: "pg_catalog".into(),
            name: "now".into(),
            arguments: String::new(),
            result: None,
            comment: None,
            kind: RoutineKind::Procedure,
        })
        .unwrap();
        assert_eq!(json["kind"], "procedure");
        assert!(json["result"].is_null());
    }
}
