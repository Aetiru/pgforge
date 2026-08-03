//! Crear, cambiar, borrar y refrescar vistas.
//!
//! Una vista no tiene columnas que se agreguen de a una, como una tabla: es un solo `SELECT` como
//! cuerpo. Por eso acá no hay nada parecido a `AddColumn` — cambiar una vista es reemplazar el
//! `SELECT` entero.
//!
//! Postgres sí tiene `CREATE OR REPLACE VIEW`, así que cambiar una vista normal es una sola
//! sentencia. Una vista materializada no admite reemplazo: cambiarla es borrarla y crearla de
//! nuevo, dos cambios en la misma transacción.
//!
//! `REFRESH MATERIALIZED VIEW`, a diferencia de `CREATE INDEX CONCURRENTLY`, sí puede correr dentro
//! de una transacción (con o sin `CONCURRENTLY`), así que todo este módulo comparte el mismo molde
//! transaccional de [`crate::ddl::table`] y no el de [`crate::ddl::index`].
//!
//! El `SELECT` es SQL crudo, misma frontera de confianza que el `default` de una columna o el
//! `CHECK` de una constraint: no se interpreta, lo ejecuta el propio usuario autenticado.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident};

use serde::Deserialize;

/// Un cambio de vista pendiente.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ViewChange {
    CreateView {
        schema: String,
        name: String,
        /// Casi siempre vacía: Postgres ya infiere los nombres de columna del `SELECT`.
        columns: Vec<String>,
        query: String,
        /// `CREATE OR REPLACE VIEW` en vez de `CREATE VIEW`.
        replace: bool,
    },
    DropView {
        schema: String,
        name: String,
        cascade: bool,
    },
    CreateMaterializedView {
        schema: String,
        name: String,
        columns: Vec<String>,
        query: String,
        /// `false` agrega `WITH NO DATA`: la vista queda vacía y no se puede leer hasta el
        /// próximo `REFRESH`.
        with_data: bool,
    },
    DropMaterializedView {
        schema: String,
        name: String,
        cascade: bool,
    },
    RefreshMaterializedView {
        schema: String,
        name: String,
        /// No bloquea a los lectores mientras se refresca, a cambio de necesitar un índice único
        /// sobre la vista.
        concurrently: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

fn column_list(columns: &[String]) -> String {
    if columns.is_empty() {
        String::new()
    } else {
        format!(
            " ({})",
            columns
                .iter()
                .map(|c| quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(changes: &[ViewChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &ViewChange) -> Result<Statement> {
    match change {
        ViewChange::CreateView {
            schema,
            name,
            columns,
            query,
            replace,
        } => {
            let query = require_query(query)?;
            Ok(statement(format!(
                "CREATE {}VIEW {}{} AS\n{query}",
                if *replace { "OR REPLACE " } else { "" },
                qualified(schema, name),
                column_list(columns)
            )))
        }
        ViewChange::DropView {
            schema,
            name,
            cascade,
        } => Ok(statement(format!(
            "DROP VIEW {}{}",
            qualified(schema, name),
            if *cascade { " CASCADE" } else { "" }
        ))),
        ViewChange::CreateMaterializedView {
            schema,
            name,
            columns,
            query,
            with_data,
        } => {
            let query = require_query(query)?;
            Ok(statement(format!(
                "CREATE MATERIALIZED VIEW {}{} AS\n{query}{}",
                qualified(schema, name),
                column_list(columns),
                if *with_data { "" } else { "\nWITH NO DATA" }
            )))
        }
        ViewChange::DropMaterializedView {
            schema,
            name,
            cascade,
        } => Ok(statement(format!(
            "DROP MATERIALIZED VIEW {}{}",
            qualified(schema, name),
            if *cascade { " CASCADE" } else { "" }
        ))),
        ViewChange::RefreshMaterializedView {
            schema,
            name,
            concurrently,
        } => Ok(statement(format!(
            "REFRESH MATERIALIZED VIEW {}{}",
            if *concurrently { "CONCURRENTLY " } else { "" },
            qualified(schema, name)
        ))),
    }
}

fn require_query(query: &str) -> Result<&str> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(Error::Config("una vista necesita una consulta".to_owned()));
    }
    Ok(trimmed)
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[ViewChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// El cuerpo del `SELECT` de una vista, sin el `CREATE VIEW ... AS` alrededor: lo que hace falta
/// para precargar el editor al abrir "Editar". Misma consulta que ya usa `ddl::view_ddl` para
/// armar el DDL completo que se muestra en el panel de detalle.
pub async fn query_of(handle: &ServerHandle, database: &str, oid: u32) -> Result<String> {
    let client = handle.client(database).await?;
    // `::oid`: `pg_get_viewdef` tiene una sobrecarga que toma `oid` y otra que toma `text`; sin el
    // cast, un parámetro sin tipo declarado se resuelve contra la de `text`, que un OID de verdad
    // no puede satisfacer. Mismo motivo que en `ddl::object_ddl`.
    let row = client
        .query_one("SELECT pg_catalog.pg_get_viewdef($1::oid, true)", &[&oid])
        .await?;
    let body: String = row.get(0);
    Ok(body.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: ViewChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn crea_una_vista_simple() {
        let statement = one_statement(ViewChange::CreateView {
            schema: "public".into(),
            name: "clientes_activos".into(),
            columns: vec![],
            query: "SELECT * FROM clientes WHERE activo".into(),
            replace: false,
        });
        assert_eq!(
            statement.sql,
            "CREATE VIEW public.clientes_activos AS\nSELECT * FROM clientes WHERE activo"
        );
    }

    #[test]
    fn crea_una_vista_con_columnas_explicitas() {
        let statement = one_statement(ViewChange::CreateView {
            schema: "public".into(),
            name: "resumen".into(),
            columns: vec!["total".into(), "cantidad".into()],
            query: "SELECT sum(monto), count(*) FROM ventas".into(),
            replace: false,
        });
        assert!(
            statement
                .sql
                .starts_with("CREATE VIEW public.resumen (total, cantidad) AS\n"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn reemplaza_una_vista() {
        let statement = one_statement(ViewChange::CreateView {
            schema: "public".into(),
            name: "clientes_activos".into(),
            columns: vec![],
            query: "SELECT * FROM clientes WHERE estado = 'activo'".into(),
            replace: true,
        });
        assert!(
            statement
                .sql
                .starts_with("CREATE OR REPLACE VIEW public.clientes_activos AS\n"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn una_vista_sin_consulta_no_se_genera() {
        assert!(statements(&[ViewChange::CreateView {
            schema: "public".into(),
            name: "x".into(),
            columns: vec![],
            query: "   ".into(),
            replace: false,
        }])
        .is_err());
    }

    #[test]
    fn borra_una_vista_con_y_sin_cascade() {
        let statement = one_statement(ViewChange::DropView {
            schema: "public".into(),
            name: "clientes_activos".into(),
            cascade: false,
        });
        assert_eq!(statement.sql, "DROP VIEW public.clientes_activos");

        let statement = one_statement(ViewChange::DropView {
            schema: "public".into(),
            name: "clientes_activos".into(),
            cascade: true,
        });
        assert_eq!(statement.sql, "DROP VIEW public.clientes_activos CASCADE");
    }

    #[test]
    fn crea_una_vista_materializada_con_with_no_data() {
        let statement = one_statement(ViewChange::CreateMaterializedView {
            schema: "public".into(),
            name: "resumen_mensual".into(),
            columns: vec![],
            query: "SELECT mes, sum(monto) FROM ventas GROUP BY mes".into(),
            with_data: false,
        });
        assert_eq!(
            statement.sql,
            "CREATE MATERIALIZED VIEW public.resumen_mensual AS\n\
             SELECT mes, sum(monto) FROM ventas GROUP BY mes\nWITH NO DATA"
        );
    }

    #[test]
    fn crea_una_vista_materializada_con_datos() {
        let statement = one_statement(ViewChange::CreateMaterializedView {
            schema: "public".into(),
            name: "resumen_mensual".into(),
            columns: vec![],
            query: "SELECT 1".into(),
            with_data: true,
        });
        assert!(!statement.sql.contains("WITH NO DATA"), "{}", statement.sql);
    }

    #[test]
    fn borra_una_vista_materializada() {
        let statement = one_statement(ViewChange::DropMaterializedView {
            schema: "public".into(),
            name: "resumen_mensual".into(),
            cascade: true,
        });
        assert_eq!(
            statement.sql,
            "DROP MATERIALIZED VIEW public.resumen_mensual CASCADE"
        );
    }

    #[test]
    fn refresca_una_vista_materializada() {
        let statement = one_statement(ViewChange::RefreshMaterializedView {
            schema: "public".into(),
            name: "resumen_mensual".into(),
            concurrently: false,
        });
        assert_eq!(
            statement.sql,
            "REFRESH MATERIALIZED VIEW public.resumen_mensual"
        );

        let statement = one_statement(ViewChange::RefreshMaterializedView {
            schema: "public".into(),
            name: "resumen_mensual".into(),
            concurrently: true,
        });
        assert_eq!(
            statement.sql,
            "REFRESH MATERIALIZED VIEW CONCURRENTLY public.resumen_mensual"
        );
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = one_statement(ViewChange::CreateView {
            schema: "mi esquema".into(),
            name: "Vista".into(),
            columns: vec!["columna uno".into()],
            query: "SELECT 1 AS \"columna uno\"".into(),
            replace: false,
        });
        assert!(
            statement
                .sql
                .contains("\"mi esquema\".\"Vista\" (\"columna uno\")"),
            "{}",
            statement.sql
        );
    }
}
