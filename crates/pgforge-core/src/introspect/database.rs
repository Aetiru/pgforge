//! La vista de una base entera: sus propiedades, sus esquemas, sus tablas y sus extensiones.
//!
//! Es lo que se pregunta al abrir una base como se abre una tabla: cómo está configurada, cuánto
//! pesa y qué tiene adentro. Todo sale de `pg_catalog` en una sola lectura por una conexión del
//! pool de **esa** base (los catálogos por base —`pg_class`, `pg_extension`— solo se ven desde
//! adentro), sin pasar por el monitor, que puede no estar abierto.
//!
//! Ninguna columna usada acá cambia dentro de PostgreSQL 13 a 17, así que no hace falta predicado
//! en `ServerCaps`. Lo que depende del rol —`numbackends` de bases ajenas se ve siempre, a
//! diferencia de `pg_stat_activity`— se leyó con ese cuidado: por eso las conexiones salen de
//! `pg_stat_database` y no de contar sesiones.

use serde::Serialize;

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

/// Cuántas tablas se traen como máximo. Una base con diez mil tablas no entra en una lista que se
/// lee a ojo; `tables_truncated` avisa que hay más y la lista viene de las más pesadas a las más
/// livianas, que es lo que importa al mirar una base.
const MAX_TABLES: i64 = 2000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseDetails {
    pub name: String,
    pub owner: String,
    pub encoding: String,
    pub collation: String,
    pub ctype: String,
    pub tablespace: String,
    pub size_bytes: i64,
    /// `-1` es «sin límite»: tal cual lo guarda `pg_database.datconnlimit`.
    pub connection_limit: i32,
    pub is_template: bool,
    pub allow_connections: bool,
    /// Sesiones abiertas ahora. `None` si el rol no puede verlo.
    pub connections: Option<i64>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSummary {
    pub name: String,
    pub owner: String,
    pub tables: i64,
    pub views: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSummary {
    pub oid: u32,
    pub schema: String,
    pub name: String,
    /// Código de `relkind`: `r` tabla, `p` particionada, `v` vista, `m` materializada, `f` externa.
    pub kind: String,
    /// Estimación de `reltuples`; `0` si todavía no se analizó (PG 14+ guarda `-1`).
    pub estimated_rows: f64,
    pub total_bytes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSummary {
    pub name: String,
    pub version: String,
    pub schema: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseOverview {
    pub details: DatabaseDetails,
    pub schemas: Vec<SchemaSummary>,
    pub tables: Vec<TableSummary>,
    /// `true` si había más tablas que [`MAX_TABLES`].
    pub tables_truncated: bool,
    pub extensions: Vec<ExtensionSummary>,
}

const DETAILS_SQL: &str = "
    SELECT d.datname::text,
           pg_catalog.pg_get_userbyid(d.datdba)::text,
           pg_catalog.pg_encoding_to_char(d.encoding)::text,
           d.datcollate::text,
           d.datctype::text,
           t.spcname::text,
           pg_catalog.pg_database_size(d.oid)::bigint,
           d.datconnlimit,
           d.datistemplate,
           d.datallowconn,
           (SELECT s.numbackends FROM pg_catalog.pg_stat_database s WHERE s.datid = d.oid)::bigint,
           pg_catalog.shobj_description(d.oid, 'pg_database')
      FROM pg_catalog.pg_database d
      JOIN pg_catalog.pg_tablespace t ON t.oid = d.dattablespace
     WHERE d.datname = $1
";

// Las particiones no cuentan: cuelgan de su madre, y una tabla con cien particiones haría de un
// esquema chico uno de cien tablas.
const SCHEMAS_SQL: &str = "
    SELECT n.nspname::text,
           pg_catalog.pg_get_userbyid(n.nspowner)::text,
           count(c.oid) FILTER (WHERE c.relkind IN ('r', 'p'))::bigint,
           count(c.oid) FILTER (WHERE c.relkind IN ('v', 'm'))::bigint
      FROM pg_catalog.pg_namespace n
      LEFT JOIN pg_catalog.pg_class c
        ON c.relnamespace = n.oid AND c.relkind IN ('r', 'p', 'v', 'm') AND NOT c.relispartition
     WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
     GROUP BY n.oid, n.nspname, n.nspowner
     ORDER BY n.nspname
";

const TABLES_SQL: &str = "
    SELECT c.oid,
           n.nspname::text,
           c.relname::text,
           c.relkind::text,
           GREATEST(c.reltuples, 0)::float8,
           pg_catalog.pg_total_relation_size(c.oid)::bigint
      FROM pg_catalog.pg_class c
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
     WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f')
       AND NOT c.relispartition
       AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
     ORDER BY 6 DESC, 2, 3
     LIMIT $1
";

const EXTENSIONS_SQL: &str = "
    SELECT e.extname::text, e.extversion::text, n.nspname::text
      FROM pg_catalog.pg_extension e
      JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace
     ORDER BY e.extname
";

/// Lee la vista completa de `database`. `Error::NotFound`-equivalente si la base no existe.
pub async fn database_overview(handle: &ServerHandle, database: &str) -> Result<DatabaseOverview> {
    let client = handle.client(database).await?;

    let row = client
        .query_opt(DETAILS_SQL, &[&database])
        .await?
        .ok_or_else(|| Error::Config(format!("la base «{database}» no existe en el servidor")))?;

    let details = DatabaseDetails {
        name: row.get(0),
        owner: row.get(1),
        encoding: row.get(2),
        collation: row.get(3),
        ctype: row.get(4),
        tablespace: row.get(5),
        size_bytes: row.get(6),
        connection_limit: row.get(7),
        is_template: row.get(8),
        allow_connections: row.get(9),
        connections: row.get(10),
        comment: row.get(11),
    };

    let schemas = client
        .query(SCHEMAS_SQL, &[])
        .await?
        .into_iter()
        .map(|row| SchemaSummary {
            name: row.get(0),
            owner: row.get(1),
            tables: row.get(2),
            views: row.get(3),
        })
        .collect();

    // Se pide una de más para saber si había más sin contar la tabla entera.
    let mut tables: Vec<TableSummary> = client
        .query(TABLES_SQL, &[&(MAX_TABLES + 1)])
        .await?
        .into_iter()
        .map(|row| TableSummary {
            oid: row.get(0),
            schema: row.get(1),
            name: row.get(2),
            kind: row.get(3),
            estimated_rows: row.get(4),
            total_bytes: row.get(5),
        })
        .collect();
    let tables_truncated = tables.len() as i64 > MAX_TABLES;
    tables.truncate(MAX_TABLES as usize);

    let extensions = client
        .query(EXTENSIONS_SQL, &[])
        .await?
        .into_iter()
        .map(|row| ExtensionSummary {
            name: row.get(0),
            version: row.get(1),
            schema: row.get(2),
        })
        .collect();

    Ok(DatabaseOverview {
        details,
        schemas,
        tables,
        tables_truncated,
        extensions,
    })
}
