//! Generación del DDL de un objeto.
//!
//! Donde PostgreSQL trae una función que ya hace el trabajo (`pg_get_viewdef`, `pg_get_indexdef`,
//! `pg_get_functiondef`, `pg_get_constraintdef`, `pg_get_triggerdef`) se usa esa, porque su salida
//! es la que el propio servidor considera correcta. Para las tablas, que no tienen equivalente, se
//! delega en `pg_dump`; ver [`pg_dump`] para el razonamiento.

pub mod pg_dump;
pub mod table;

pub use table::{ColumnDef, Identity, Statement as TableStatement, TableChange};

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};
use crate::introspect::{NodeKind, TreeNode};

/// De dónde salió el DDL. La interfaz lo muestra porque no es lo mismo lo que afirma el servidor
/// que lo que reconstruyó una herramienta externa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DdlSource {
    /// Generado por una función del propio PostgreSQL.
    Catalog,
    /// Producido por `pg_dump`.
    PgDump,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ddl {
    pub sql: String,
    pub source: DdlSource,
}

impl Ddl {
    fn catalog(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            source: DdlSource::Catalog,
        }
    }
}

/// Cita un identificador solo si hace falta, igual que `quote_ident` del servidor. Citar siempre
/// produce un DDL correcto pero incómodo de leer.
pub fn quote_ident(ident: &str) -> String {
    const RESERVED: &[&str] = &[
        "all",
        "and",
        "any",
        "array",
        "as",
        "asc",
        "between",
        "by",
        "case",
        "cast",
        "check",
        "column",
        "constraint",
        "create",
        "default",
        "desc",
        "distinct",
        "do",
        "else",
        "end",
        "except",
        "false",
        "for",
        "foreign",
        "from",
        "grant",
        "group",
        "having",
        "in",
        "index",
        "inner",
        "into",
        "is",
        "join",
        "left",
        "like",
        "limit",
        "not",
        "null",
        "offset",
        "on",
        "or",
        "order",
        "outer",
        "primary",
        "references",
        "right",
        "select",
        "table",
        "then",
        "to",
        "true",
        "union",
        "unique",
        "update",
        "user",
        "using",
        "when",
        "where",
        "with",
    ];

    let simple = !ident.is_empty()
        && !ident.starts_with(|c: char| c.is_ascii_digit())
        && ident
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');

    if simple && !RESERVED.contains(&ident) {
        ident.to_owned()
    } else {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }
}

pub(crate) fn qualified(schema: &str, name: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(name))
}

/// Devuelve el DDL del objeto que representa el nodo.
pub async fn object_ddl(handle: &ServerHandle, node: &TreeNode) -> Result<Ddl> {
    match node.kind {
        NodeKind::Table | NodeKind::PartitionedTable | NodeKind::ForeignTable => {
            table_ddl(handle, node).await
        }
        NodeKind::View => view_ddl(handle, node, false).await,
        NodeKind::MaterializedView => view_ddl(handle, node, true).await,
        NodeKind::Index => simple_def(handle, node, "pg_catalog.pg_get_indexdef($1)").await,
        NodeKind::Trigger => {
            simple_def(handle, node, "pg_catalog.pg_get_triggerdef($1, true)").await
        }
        NodeKind::Function | NodeKind::Procedure => {
            simple_def(handle, node, "pg_catalog.pg_get_functiondef($1)").await
        }
        NodeKind::Constraint => constraint_ddl(handle, node).await,
        NodeKind::Sequence => sequence_ddl(handle, node).await,
        NodeKind::Type => type_ddl(handle, node).await,
        NodeKind::Column => column_ddl(handle, node).await,
        NodeKind::Schema => schema_ddl(handle, node).await,
        NodeKind::Database | NodeKind::Folder(_) => {
            Err(Error::Config("este nodo no tiene un DDL propio".to_owned()))
        }
    }
}

async fn table_ddl(handle: &ServerHandle, node: &TreeNode) -> Result<Ddl> {
    let schema = node
        .schema
        .as_deref()
        .ok_or_else(|| Error::Config("la tabla no tiene esquema asociado".to_owned()))?;

    let sql = pg_dump::dump_object(
        &handle.profile,
        handle.password(),
        &node.database,
        &pg_dump::table_pattern(schema, &node.label),
    )
    .await?;

    Ok(Ddl {
        sql,
        source: DdlSource::PgDump,
    })
}

async fn view_ddl(handle: &ServerHandle, node: &TreeNode, materialized: bool) -> Result<Ddl> {
    let oid = require_oid(node)?;
    let client = handle.client(&node.database).await?;
    let row = client
        .query_one("SELECT pg_catalog.pg_get_viewdef($1, true)", &[&oid])
        .await?;
    let body: String = row.get(0);

    let name = qualified(node.schema.as_deref().unwrap_or("public"), &node.label);
    let keyword = if materialized {
        "CREATE MATERIALIZED VIEW"
    } else {
        "CREATE OR REPLACE VIEW"
    };

    Ok(Ddl::catalog(format!(
        "{keyword} {name} AS\n{}",
        body.trim_end()
    )))
}

/// Para los objetos cuyo DDL completo lo devuelve una sola función del servidor.
async fn simple_def(handle: &ServerHandle, node: &TreeNode, expression: &str) -> Result<Ddl> {
    let oid = require_oid(node)?;
    let client = handle.client(&node.database).await?;
    let row = client
        .query_one(&format!("SELECT {expression}"), &[&oid])
        .await?;
    let definition: Option<String> = row.get(0);

    let definition = definition
        .ok_or_else(|| Error::Config("el servidor no devolvió una definición".to_owned()))?;

    let definition = definition.trim_end().to_owned();
    let sql = if definition.ends_with(';') {
        definition
    } else {
        format!("{definition};")
    };

    Ok(Ddl::catalog(sql))
}

async fn constraint_ddl(handle: &ServerHandle, node: &TreeNode) -> Result<Ddl> {
    let oid = require_oid(node)?;
    let client = handle.client(&node.database).await?;
    let row = client
        .query_one(
            "SELECT con.conrelid::regclass::text,
                    con.conname::text,
                    pg_catalog.pg_get_constraintdef(con.oid, true)
               FROM pg_catalog.pg_constraint con
              WHERE con.oid = $1",
            &[&oid],
        )
        .await?;

    let table: String = row.get(0);
    let name: String = row.get(1);
    let definition: String = row.get(2);

    Ok(Ddl::catalog(format!(
        "ALTER TABLE {table}\n    ADD CONSTRAINT {} {definition};",
        quote_ident(&name)
    )))
}

async fn sequence_ddl(handle: &ServerHandle, node: &TreeNode) -> Result<Ddl> {
    let oid = require_oid(node)?;
    let client = handle.client(&node.database).await?;
    let row = client
        .query_one(
            "SELECT pg_catalog.format_type(s.seqtypid, NULL),
                    s.seqstart, s.seqincrement, s.seqmin, s.seqmax, s.seqcache, s.seqcycle,
                    pg_catalog.pg_get_serial_sequence(
                        c.oid::regclass::text, a.attname)
               FROM pg_catalog.pg_sequence s
               JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid
               LEFT JOIN pg_catalog.pg_depend d
                      ON d.objid = s.seqrelid AND d.deptype IN ('a', 'i')
               LEFT JOIN pg_catalog.pg_attribute a
                      ON a.attrelid = d.refobjid AND a.attnum = d.refobjsubid
              WHERE s.seqrelid = $1",
            &[&oid],
        )
        .await?;

    let type_name: String = row.get(0);
    let start: i64 = row.get(1);
    let increment: i64 = row.get(2);
    let min: i64 = row.get(3);
    let max: i64 = row.get(4);
    let cache: i64 = row.get(5);
    let cycle: bool = row.get(6);
    let owned_by: Option<String> = row.get(7);

    let name = qualified(node.schema.as_deref().unwrap_or("public"), &node.label);
    let mut sql = format!(
        "CREATE SEQUENCE {name}\n    AS {type_name}\n    START WITH {start}\n    \
         INCREMENT BY {increment}\n    MINVALUE {min}\n    MAXVALUE {max}\n    CACHE {cache}"
    );
    if cycle {
        sql.push_str("\n    CYCLE");
    }
    sql.push(';');

    // Una secuencia creada por una columna `serial` o `identity` pertenece a esa columna: sin esta
    // línea, recrearla dejaría la columna sin su secuencia.
    if let Some(owned_by) = owned_by {
        sql.push_str(&format!("\n\nALTER SEQUENCE {name} OWNED BY {owned_by};"));
    }

    Ok(Ddl::catalog(sql))
}

async fn type_ddl(handle: &ServerHandle, node: &TreeNode) -> Result<Ddl> {
    let oid = require_oid(node)?;
    let client = handle.client(&node.database).await?;
    let row = client
        .query_one(
            "SELECT t.typtype::text,
                    pg_catalog.format_type(t.typbasetype, t.typtypmod),
                    t.typnotnull,
                    t.typdefault
               FROM pg_catalog.pg_type t
              WHERE t.oid = $1",
            &[&oid],
        )
        .await?;

    let typtype: String = row.get(0);
    let name = qualified(node.schema.as_deref().unwrap_or("public"), &node.label);

    let sql = match typtype.as_str() {
        "e" => {
            let rows = client
                .query(
                    "SELECT e.enumlabel::text
                       FROM pg_catalog.pg_enum e
                      WHERE e.enumtypid = $1
                      ORDER BY e.enumsortorder",
                    &[&oid],
                )
                .await?;
            let labels = rows
                .iter()
                .map(|row| format!("    '{}'", row.get::<_, String>(0).replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",\n");
            format!("CREATE TYPE {name} AS ENUM (\n{labels}\n);")
        }
        "c" => {
            let rows = client
                .query(
                    "SELECT a.attname::text,
                            pg_catalog.format_type(a.atttypid, a.atttypmod)
                       FROM pg_catalog.pg_attribute a
                       JOIN pg_catalog.pg_type t ON t.typrelid = a.attrelid
                      WHERE t.oid = $1 AND a.attnum > 0 AND NOT a.attisdropped
                      ORDER BY a.attnum",
                    &[&oid],
                )
                .await?;
            let fields = rows
                .iter()
                .map(|row| {
                    format!(
                        "    {} {}",
                        quote_ident(&row.get::<_, String>(0)),
                        row.get::<_, String>(1)
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            format!("CREATE TYPE {name} AS (\n{fields}\n);")
        }
        "d" => {
            let base: String = row.get(1);
            let not_null: bool = row.get(2);
            let default: Option<String> = row.get(3);

            let mut sql = format!("CREATE DOMAIN {name} AS {base}");
            if let Some(default) = default {
                sql.push_str(&format!("\n    DEFAULT {default}"));
            }
            if not_null {
                sql.push_str("\n    NOT NULL");
            }

            let checks = client
                .query(
                    "SELECT pg_catalog.pg_get_constraintdef(con.oid, true)
                       FROM pg_catalog.pg_constraint con
                      WHERE con.contypid = $1
                      ORDER BY con.conname",
                    &[&oid],
                )
                .await?;
            for check in &checks {
                sql.push_str(&format!("\n    {}", check.get::<_, String>(0)));
            }
            sql.push(';');
            sql
        }
        "r" | "m" => {
            let subtype = client
                .query_one(
                    "SELECT pg_catalog.format_type(r.rngsubtype, NULL)
                       FROM pg_catalog.pg_range r
                      WHERE r.rngtypid = $1",
                    &[&oid],
                )
                .await?;
            format!(
                "CREATE TYPE {name} AS RANGE (\n    subtype = {}\n);",
                subtype.get::<_, String>(0)
            )
        }
        other => {
            return Err(Error::Config(format!(
                "todavía no se genera el DDL de los tipos de categoría '{other}'"
            )))
        }
    };

    Ok(Ddl::catalog(sql))
}

async fn column_ddl(handle: &ServerHandle, node: &TreeNode) -> Result<Ddl> {
    let oid = require_oid(node)?;
    let client = handle.client(&node.database).await?;
    let row = client
        .query_one(
            "SELECT a.attrelid::regclass::text,
                    pg_catalog.format_type(a.atttypid, a.atttypmod),
                    a.attnotnull,
                    pg_catalog.pg_get_expr(d.adbin, d.adrelid)
               FROM pg_catalog.pg_attribute a
               LEFT JOIN pg_catalog.pg_attrdef d
                      ON d.adrelid = a.attrelid AND d.adnum = a.attnum
              WHERE a.attrelid = $1 AND a.attname = $2",
            &[&oid, &node.label],
        )
        .await?;

    let table: String = row.get(0);
    let type_name: String = row.get(1);
    let not_null: bool = row.get(2);
    let default: Option<String> = row.get(3);

    let mut sql = format!(
        "ALTER TABLE {table}\n    ADD COLUMN {} {type_name}",
        quote_ident(&node.label)
    );
    if not_null {
        sql.push_str(" NOT NULL");
    }
    if let Some(default) = default {
        sql.push_str(&format!(" DEFAULT {default}"));
    }
    sql.push(';');

    Ok(Ddl::catalog(sql))
}

async fn schema_ddl(handle: &ServerHandle, node: &TreeNode) -> Result<Ddl> {
    let oid = require_oid(node)?;
    let client = handle.client(&node.database).await?;
    let row = client
        .query_one(
            "SELECT pg_catalog.pg_get_userbyid(n.nspowner)::text
               FROM pg_catalog.pg_namespace n
              WHERE n.oid = $1",
            &[&oid],
        )
        .await?;

    Ok(Ddl::catalog(format!(
        "CREATE SCHEMA {} AUTHORIZATION {};",
        quote_ident(&node.label),
        quote_ident(&row.get::<_, String>(0))
    )))
}

fn require_oid(node: &TreeNode) -> Result<u32> {
    node.oid
        .ok_or_else(|| Error::Config("el nodo no tiene OID asociado".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cita_solo_lo_necesario() {
        assert_eq!(quote_ident("usuarios"), "usuarios");
        assert_eq!(quote_ident("tabla_1"), "tabla_1");
        assert_eq!(quote_ident("Usuarios"), "\"Usuarios\"");
        assert_eq!(quote_ident("mi tabla"), "\"mi tabla\"");
        assert_eq!(quote_ident("1tabla"), "\"1tabla\"");
        assert_eq!(quote_ident("co\"millas"), "\"co\"\"millas\"");
        // Una palabra reservada sin citar rompe la sentencia.
        assert_eq!(quote_ident("order"), "\"order\"");
        assert_eq!(quote_ident("user"), "\"user\"");
    }
}
