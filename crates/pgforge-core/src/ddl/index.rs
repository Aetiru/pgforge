//! Crear y borrar índices.
//!
//! A diferencia de todo lo demás en [`crate::ddl::table`], las operaciones de acá nunca se agrupan
//! en una transacción ni se agrupan entre sí: `CREATE INDEX CONCURRENTLY` y
//! `DROP INDEX CONCURRENTLY` **no pueden correr dentro de un bloque de transacción**, así que cada
//! sentencia se manda sola, en su propia conexión. No cambia nada para el caso sin `CONCURRENTLY`
//! — una sola sentencia DDL ya es atómica por sí misma —, pero sí significa que no existe acá un
//! `apply()` que reciba un lote: crear tres índices son tres llamadas, no una.
//!
//! Postgres tampoco tiene un "ALTER INDEX" que cambie qué columnas cubre o su predicado: cambiar
//! un índice es borrarlo y crear uno nuevo.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::{qualified, quote_ident, table::Statement};

/// Un índice nuevo.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDef {
    pub schema: String,
    pub table: String,
    /// Si no se da, Postgres lo nombra solo.
    pub name: Option<String>,
    pub unique: bool,
    /// `btree` si no se da.
    pub method: Option<String>,
    pub columns: Vec<String>,
    /// Predicado crudo de un índice parcial (`WHERE ...`).
    pub where_clause: Option<String>,
    /// No bloquea la tabla mientras se construye, a costa de no poder correr en una transacción.
    pub concurrently: bool,
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// El `CREATE INDEX` para `def`. Puro: es lo que alimenta la vista previa.
pub fn create_sql(def: &IndexDef) -> Result<Statement> {
    if def.columns.is_empty() {
        return Err(Error::Config(
            "un índice necesita al menos una columna".to_owned(),
        ));
    }

    let mut sql = "CREATE ".to_owned();
    if def.unique {
        sql.push_str("UNIQUE ");
    }
    sql.push_str("INDEX ");
    if def.concurrently {
        sql.push_str("CONCURRENTLY ");
    }
    if let Some(name) = &def.name {
        if !name.trim().is_empty() {
            sql.push_str(&quote_ident(name));
            sql.push(' ');
        }
    }
    sql.push_str(&format!("ON {}", qualified(&def.schema, &def.table)));
    if let Some(method) = &def.method {
        if !method.trim().is_empty() {
            sql.push_str(&format!(" USING {method}"));
        }
    }
    let columns = def
        .columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    sql.push_str(&format!(" ({columns})"));

    if let Some(predicate) = &def.where_clause {
        if !predicate.trim().is_empty() {
            sql.push_str(&format!(" WHERE {predicate}"));
        }
    }

    Ok(statement(sql))
}

/// Crea el índice. Sin transacción: ver el comentario del módulo.
pub async fn create(handle: &ServerHandle, database: &str, def: &IndexDef) -> Result<()> {
    let statement = create_sql(def)?;
    let client = handle.client(database).await?;
    client.batch_execute(&statement.sql).await?;
    Ok(())
}

/// El `DROP INDEX` para borrar `name`. Puro, y rechaza `cascade` junto con `concurrently`: Postgres
/// tampoco lo permite, y sin esta verificación el error solo aparecería al ejecutar.
pub fn drop_sql(schema: &str, name: &str, cascade: bool, concurrently: bool) -> Result<Statement> {
    if cascade && concurrently {
        return Err(Error::Config(
            "no se puede borrar un índice con CASCADE y CONCURRENTLY a la vez".to_owned(),
        ));
    }

    let mut sql = "DROP INDEX ".to_owned();
    if concurrently {
        sql.push_str("CONCURRENTLY ");
    }
    sql.push_str(&qualified(schema, name));
    if cascade {
        sql.push_str(" CASCADE");
    }

    Ok(statement(sql))
}

/// Borra el índice. Sin transacción, mismo motivo que [`create`].
pub async fn drop_index(
    handle: &ServerHandle,
    database: &str,
    schema: &str,
    name: &str,
    cascade: bool,
    concurrently: bool,
) -> Result<()> {
    let statement = drop_sql(schema, name, cascade, concurrently)?;
    let client = handle.client(database).await?;
    client.batch_execute(&statement.sql).await?;
    Ok(())
}

/// Un índice tal como ya existe, para mostrarlo y ofrecer borrarlo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub oid: u32,
    pub name: String,
    pub definition: String,
    pub primary: bool,
    pub unique: bool,
    /// `false` si quedó de un `CREATE INDEX CONCURRENTLY` que falló: el planificador no lo usa.
    pub valid: bool,
    pub method: String,
}

const INDEXES_SQL: &str = "
    SELECT c.oid,
           c.relname::text,
           i.indisprimary,
           i.indisunique,
           i.indisvalid,
           am.amname::text,
           pg_catalog.pg_get_indexdef(c.oid)
      FROM pg_catalog.pg_index i
      JOIN pg_catalog.pg_class c ON c.oid = i.indexrelid
      JOIN pg_catalog.pg_am am ON am.oid = c.relam
     WHERE i.indrelid = $1
     ORDER BY c.relname
";

/// Los índices que ya tiene una tabla. Misma consulta que usa el árbol para mostrarlos
/// (`introspect::indexes`), con `pg_get_indexdef` sumado para traer la definición completa.
pub async fn indexes(handle: &ServerHandle, database: &str, oid: u32) -> Result<Vec<IndexInfo>> {
    let client = handle.client(database).await?;
    let rows = client.query(INDEXES_SQL, &[&oid]).await?;

    Ok(rows
        .into_iter()
        .map(|row| IndexInfo {
            oid: row.get(0),
            name: row.get(1),
            primary: row.get(2),
            unique: row.get(3),
            valid: row.get(4),
            method: row.get(5),
            definition: row.get(6),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> IndexDef {
        IndexDef {
            schema: "public".into(),
            table: "clientes".into(),
            name: None,
            unique: false,
            method: None,
            columns: vec!["nombre".into()],
            where_clause: None,
            concurrently: false,
        }
    }

    #[test]
    fn crea_un_indice_simple_sin_nombre() {
        let statement = create_sql(&def()).unwrap();
        assert_eq!(statement.sql, "CREATE INDEX ON public.clientes (nombre)");
    }

    #[test]
    fn crea_un_indice_con_nombre_unico_y_metodo() {
        let mut d = def();
        d.name = Some("clientes_nombre_idx".into());
        d.unique = true;
        d.method = Some("btree".into());
        let statement = create_sql(&d).unwrap();
        assert_eq!(
            statement.sql,
            "CREATE UNIQUE INDEX clientes_nombre_idx ON public.clientes USING btree (nombre)"
        );
    }

    #[test]
    fn crea_un_indice_parcial() {
        let mut d = def();
        d.where_clause = Some("nombre IS NOT NULL".into());
        let statement = create_sql(&d).unwrap();
        assert_eq!(
            statement.sql,
            "CREATE INDEX ON public.clientes (nombre) WHERE nombre IS NOT NULL"
        );
    }

    #[test]
    fn crea_un_indice_concurrently() {
        let mut d = def();
        d.concurrently = true;
        let statement = create_sql(&d).unwrap();
        assert_eq!(
            statement.sql,
            "CREATE INDEX CONCURRENTLY ON public.clientes (nombre)"
        );
    }

    #[test]
    fn un_indice_sin_columnas_no_se_genera() {
        let mut d = def();
        d.columns = vec![];
        assert!(create_sql(&d).is_err());
    }

    #[test]
    fn columnas_compuestas_en_orden() {
        let mut d = def();
        d.columns = vec!["apellido".into(), "nombre".into()];
        let statement = create_sql(&d).unwrap();
        assert!(
            statement.sql.contains("(apellido, nombre)"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn borra_un_indice_simple() {
        let statement = drop_sql("public", "clientes_nombre_idx", false, false).unwrap();
        assert_eq!(statement.sql, "DROP INDEX public.clientes_nombre_idx");
    }

    #[test]
    fn borra_un_indice_con_cascade() {
        let statement = drop_sql("public", "clientes_nombre_idx", true, false).unwrap();
        assert_eq!(
            statement.sql,
            "DROP INDEX public.clientes_nombre_idx CASCADE"
        );
    }

    #[test]
    fn borra_un_indice_concurrently() {
        let statement = drop_sql("public", "clientes_nombre_idx", false, true).unwrap();
        assert_eq!(
            statement.sql,
            "DROP INDEX CONCURRENTLY public.clientes_nombre_idx"
        );
    }

    #[test]
    fn cascade_y_concurrently_juntos_no_se_generan() {
        assert!(drop_sql("public", "clientes_nombre_idx", true, true).is_err());
    }
}
