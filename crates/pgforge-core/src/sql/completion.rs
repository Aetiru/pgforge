//! Nombres del esquema para el autocompletado del editor.
//!
//! Se trae todo de una y se cachea del lado de la interfaz, en vez de consultar el catálogo por
//! cada tecla: son unos pocos miles de nombres, y el editor tiene que responder mientras se
//! escribe.
//!
//! No reusa `introspect` a propósito: ese recorre el árbol por niveles y devuelve `TreeNode`, que
//! es lo que necesita el explorador. Acá hace falta lo contrario, todo plano y de una sola vez.

use serde::Serialize;

use crate::conn::ServerHandle;
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    pub schema: String,
    pub name: String,
    pub columns: Vec<String>,
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
    // el tamaño de la respuesta para armar exactamente lo mismo de este lado.
    let relations = client
        .query(
            &format!(
                "SELECT n.nspname::text,
                        c.relname::text,
                        coalesce(
                            array_agg(a.attname::text ORDER BY a.attnum)
                                FILTER (WHERE a.attnum > 0 AND NOT a.attisdropped),
                            '{{}}'
                        )
                   FROM pg_catalog.pg_class c
                   JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                   LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid
                  WHERE c.relkind = ANY (ARRAY['r', 'p', 'v', 'm', 'f'])
                    AND {VISIBLE_SCHEMAS}
                    AND pg_catalog.has_table_privilege(c.oid, 'SELECT')
                  GROUP BY n.nspname, c.relname
                  ORDER BY n.nspname, c.relname"
            ),
            &[],
        )
        .await?
        .into_iter()
        .map(|row| Relation {
            schema: row.get(0),
            name: row.get(1),
            columns: row.get(2),
        })
        .collect();

    Ok(SchemaSnapshot {
        database: database.to_owned(),
        schemas,
        relations,
    })
}
