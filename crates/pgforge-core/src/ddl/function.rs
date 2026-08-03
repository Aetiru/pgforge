//! Crear, cambiar y borrar funciones y procedimientos.
//!
//! Acá no hay nada parecido a `TableChange` o `ViewChange`: los argumentos con `OUT`/`VARIADIC`/
//! valores por omisión, el tipo de retorno (escalar o `TABLE(...)`), el lenguaje, la volatilidad,
//! `STRICT`, `SECURITY DEFINER`... todo eso vive entrelazado en una sola sentencia `CREATE
//! FUNCTION`. Reconstruirlo en campos separados sería más frágil que la sentencia misma, y peor
//! herramienta que el editor SQL de Fase 3, que ya ejecuta cualquier DDL. Por eso acá se ejecuta el
//! texto tal cual lo escribió el usuario — la misma frontera de confianza que ya tienen el
//! `default` de una columna o el `CHECK` de una constraint, solo que acá es la sentencia entera.
//!
//! `pg_get_functiondef` (que ya usa [`crate::ddl::simple_def`] para mostrar el DDL) devuelve un
//! `CREATE OR REPLACE FUNCTION` completo: reabrir ese texto, cambiarlo y volver a ejecutarlo ya
//! reemplaza la función. No hace falta que este módulo sepa si es una creación o un reemplazo.
//!
//! Lo único que sí hace falta resolver es borrar: `DROP FUNCTION` necesita la lista de tipos de
//! los argumentos para desambiguar entre sobrecargas del mismo nombre.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::qualified;
use super::table::Statement;

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// `FUNCTION` o `PROCEDURE`. Desde PG 11 no son intercambiables: un `DROP FUNCTION` no borra un
/// procedimiento y un `GRANT ... ON FUNCTION` no lo alcanza, así que [`super::privilege`] usa la
/// misma palabra que se usa acá.
pub(super) fn keyword(procedure: bool) -> &'static str {
    if procedure {
        "PROCEDURE"
    } else {
        "FUNCTION"
    }
}

/// Valida la sentencia antes de ejecutarla. Es lo único que se puede verificar sin servidor: el
/// resto (que sea SQL válido, que el lenguaje exista) lo valida Postgres al correrla.
pub fn apply_sql(sql: &str) -> Result<Statement> {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return Err(Error::Config(
            "hace falta una sentencia CREATE FUNCTION o CREATE PROCEDURE".to_owned(),
        ));
    }
    Ok(statement(trimmed.to_owned()))
}

/// Ejecuta la sentencia tal cual. Sin transacción explícita: es una sola sentencia, ya es atómica
/// por sí sola.
pub async fn apply(handle: &ServerHandle, database: &str, sql: &str) -> Result<()> {
    let statement = apply_sql(sql)?;
    let client = handle.client(database).await?;
    client.batch_execute(&statement.sql).await?;
    Ok(())
}

/// El `DROP FUNCTION`/`DROP PROCEDURE` para borrar `name`. `args` no se cita ni se interpreta: sale
/// ya formateado del servidor (ver [`identity_args`]), igual que `type_name` en
/// [`crate::data::shape::Column`].
pub fn drop_sql(
    schema: &str,
    name: &str,
    args: &str,
    procedure: bool,
    cascade: bool,
) -> Result<Statement> {
    Ok(statement(format!(
        "DROP {} {}({args}){}",
        keyword(procedure),
        qualified(schema, name),
        if cascade { " CASCADE" } else { "" }
    )))
}

/// Borra la función o el procedimiento.
pub async fn drop(
    handle: &ServerHandle,
    database: &str,
    schema: &str,
    name: &str,
    args: &str,
    procedure: bool,
    cascade: bool,
) -> Result<()> {
    let statement = drop_sql(schema, name, args, procedure, cascade)?;
    let client = handle.client(database).await?;
    client.batch_execute(&statement.sql).await?;
    Ok(())
}

/// La lista de tipos de argumento tal como la necesita `DROP FUNCTION`/`DROP PROCEDURE`. Mismo
/// `pg_get_function_identity_arguments` que ya usa `introspect::routines` para mostrar el árbol.
pub async fn identity_args(handle: &ServerHandle, database: &str, oid: u32) -> Result<String> {
    let client = handle.client(database).await?;
    let row = client
        .query_one(
            "SELECT pg_catalog.pg_get_function_identity_arguments($1)",
            &[&oid],
        )
        .await?;
    Ok(row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borra_una_funcion_con_y_sin_cascade() {
        let statement = drop_sql("public", "totalizar", "integer, text", false, false).unwrap();
        assert_eq!(
            statement.sql,
            "DROP FUNCTION public.totalizar(integer, text)"
        );

        let statement = drop_sql("public", "totalizar", "integer, text", false, true).unwrap();
        assert_eq!(
            statement.sql,
            "DROP FUNCTION public.totalizar(integer, text) CASCADE"
        );
    }

    #[test]
    fn borra_un_procedimiento() {
        let statement = drop_sql("public", "archivar", "", true, false).unwrap();
        assert_eq!(statement.sql, "DROP PROCEDURE public.archivar()");
    }

    #[test]
    fn una_sentencia_vacia_no_se_aplica() {
        assert!(apply_sql("   ").is_err());
    }

    #[test]
    fn una_sentencia_con_contenido_se_acepta_tal_cual() {
        let sql = "CREATE FUNCTION public.f() RETURNS void LANGUAGE sql AS $$ SELECT 1 $$;";
        let statement = apply_sql(sql).unwrap();
        assert_eq!(statement.sql, sql);
    }
}
