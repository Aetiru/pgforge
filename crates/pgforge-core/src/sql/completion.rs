//! Nombres del esquema para el autocompletado del editor.
//!
//! Se trae todo de una y se cachea del lado de la interfaz, en vez de consultar el catálogo por
//! cada tecla: son unos pocos miles de nombres, y el editor tiene que responder mientras se
//! escribe.
//!
//! No reusa `introspect` a propósito: ese recorre el árbol por niveles y devuelve `TreeNode`, que
//! es lo que necesita el explorador. Acá hace falta lo contrario, todo plano y de una sola vez.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::Result;

/// Nombre pelado más lo que hace falta para el `Ctrl`+clic que revela la tabla en el árbol y el
/// `hover` que muestra el tipo y el comentario de una columna.
///
/// `Deserialize` es para leer el `jsonb_agg` que arma `snapshot()` del lado del servidor: son las
/// mismas tres claves, así que ida y vuelta no pide nada aparte (mismo molde que
/// `data::shape::Column`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationColumn {
    pub name: String,
    /// Tal como lo escribe `format_type`, igual que en `data::shape::Column`.
    pub type_name: String,
    /// `None` es el caso normal: no vale la pena mandar `null` por cada una de las columnas de una
    /// base que casi nunca comenta ninguna.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    /// Para el `Ctrl`+clic que la revela en el árbol: `explorer.revealRelation` la pide.
    pub oid: u32,
    pub schema: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub columns: Vec<RelationColumn>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSnapshot {
    pub database: String,
    pub schemas: Vec<String>,
    pub relations: Vec<Relation>,
}

/// Los esquemas del sistema quedan afuera aunque el explorador los esté mostrando: `pg_catalog`
/// solo aporta miles de nombres que nadie escribe a mano y que taparían a los propios.
const VISIBLE_SCHEMAS: &str = "
    n.nspname NOT IN ('pg_catalog', 'information_schema')
    AND n.nspname NOT LIKE 'pg\\_toast%'
    AND n.nspname NOT LIKE 'pg\\_temp%'
    AND pg_catalog.has_schema_privilege(n.oid, 'USAGE')
";

/// Nombres visibles de una base, para alimentar el autocompletado.
pub async fn snapshot(handle: &ServerHandle, database: &str) -> Result<SchemaSnapshot> {
    let client = handle.client(database).await?;

    let schemas = client
        .query(
            &format!(
                "SELECT n.nspname::text
                   FROM pg_catalog.pg_namespace n
                  WHERE {VISIBLE_SCHEMAS}
                  ORDER BY n.nspname"
            ),
            &[],
        )
        .await?
        .into_iter()
        .map(|row| row.get(0))
        .collect();

    // Las columnas se agregan en el servidor: traerlas como filas sueltas multiplicaría por veinte
    // el tamaño de la respuesta para armar exactamente lo mismo de este lado. Van como JSON y no
    // como un array compuesto porque `tokio-postgres` no trae decodificador para eso, y el texto de
    // un `jsonb_agg` es la misma solución que ya usa `sql::explain` para el plan.
    let relations = client
        .query(
            &format!(
                "SELECT n.nspname::text,
                        c.relname::text,
                        c.oid,
                        obj_description(c.oid, 'pg_class'),
                        coalesce(
                            jsonb_agg(
                                jsonb_build_object(
                                    'name', a.attname,
                                    'typeName', pg_catalog.format_type(a.atttypid, a.atttypmod),
                                    'comment', pg_catalog.col_description(c.oid, a.attnum)
                                )
                                ORDER BY a.attnum
                            ) FILTER (WHERE a.attnum > 0 AND NOT a.attisdropped),
                            '[]'
                        )::text
                   FROM pg_catalog.pg_class c
                   JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                   LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid
                  WHERE c.relkind = ANY (ARRAY['r', 'p', 'v', 'm', 'f'])
                    AND {VISIBLE_SCHEMAS}
                    AND pg_catalog.has_table_privilege(c.oid, 'SELECT')
                  GROUP BY n.nspname, c.relname, c.oid
                  ORDER BY n.nspname, c.relname"
            ),
            &[],
        )
        .await?
        .into_iter()
        .map(|row| {
            let columns_json: String = row.get(4);
            let columns: Vec<RelationColumn> =
                serde_json::from_str(&columns_json).unwrap_or_default();
            Relation {
                oid: row.get(2),
                schema: row.get(0),
                name: row.get(1),
                comment: row.get(3),
                columns,
            }
        })
        .collect();

    Ok(SchemaSnapshot {
        database: database.to_owned(),
        schemas,
        relations,
    })
}
