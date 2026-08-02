//! Otorgar y revocar privilegios sobre tablas y esquemas.
//!
//! Postgres tiene un vocabulario de privilegios distinto por tipo de objeto — una tabla admite
//! `SELECT`/`INSERT`/..., un esquema solo `USAGE`/`CREATE` — así que acá no hay un enum genérico de
//! "privilegio", sino uno por tipo de objeto. `GRANT ALL PRIVILEGES` no es un caso aparte: es
//! exactamente lo mismo que listar todos los privilegios de ese tipo, así que un checkbox "Todos"
//! en la interfaz alcanza con marcarlos todos, sin que el núcleo necesite saber que eso es "todos".

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::quote_ident;
use super::table::Statement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TablePrivilege {
    Select,
    Insert,
    Update,
    Delete,
    Truncate,
    References,
    Trigger,
}

impl TablePrivilege {
    fn sql(self) -> &'static str {
        match self {
            TablePrivilege::Select => "SELECT",
            TablePrivilege::Insert => "INSERT",
            TablePrivilege::Update => "UPDATE",
            TablePrivilege::Delete => "DELETE",
            TablePrivilege::Truncate => "TRUNCATE",
            TablePrivilege::References => "REFERENCES",
            TablePrivilege::Trigger => "TRIGGER",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SchemaPrivilege {
    Usage,
    Create,
}

impl SchemaPrivilege {
    fn sql(self) -> &'static str {
        match self {
            SchemaPrivilege::Usage => "USAGE",
            SchemaPrivilege::Create => "CREATE",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PrivilegeChange {
    GrantTable {
        schema: String,
        table: String,
        privileges: Vec<TablePrivilege>,
        grantee: String,
        grant_option: bool,
    },
    RevokeTable {
        schema: String,
        table: String,
        privileges: Vec<TablePrivilege>,
        grantee: String,
        /// `REVOKE GRANT OPTION FOR ...`: revoca solo el permiso de volver a otorgar, no el
        /// privilegio en sí.
        grant_option_only: bool,
        cascade: bool,
    },
    GrantSchema {
        schema: String,
        privileges: Vec<SchemaPrivilege>,
        grantee: String,
        grant_option: bool,
    },
    RevokeSchema {
        schema: String,
        privileges: Vec<SchemaPrivilege>,
        grantee: String,
        grant_option_only: bool,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// `PUBLIC` es una palabra clave, no un identificador: citarla generaría un rol llamado
/// literalmente `"PUBLIC"`, que no es lo mismo que el pseudo-rol especial de Postgres.
fn grantee_sql(name: &str) -> String {
    if name.eq_ignore_ascii_case("public") {
        "PUBLIC".to_owned()
    } else {
        quote_ident(name)
    }
}

fn table_name(schema: &str, table: &str) -> String {
    format!("{}.{}", quote_ident(schema), quote_ident(table))
}

pub fn statements(changes: &[PrivilegeChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &PrivilegeChange) -> Result<Statement> {
    match change {
        PrivilegeChange::GrantTable {
            schema,
            table,
            privileges,
            grantee,
            grant_option,
        } => {
            let list = require_privileges(privileges, TablePrivilege::sql)?;
            Ok(statement(format!(
                "GRANT {list} ON {} TO {}{}",
                table_name(schema, table),
                grantee_sql(grantee),
                if *grant_option { " WITH GRANT OPTION" } else { "" }
            )))
        }
        PrivilegeChange::RevokeTable {
            schema,
            table,
            privileges,
            grantee,
            grant_option_only,
            cascade,
        } => {
            let list = require_privileges(privileges, TablePrivilege::sql)?;
            Ok(statement(format!(
                "REVOKE {}{list} ON {} FROM {}{}",
                if *grant_option_only { "GRANT OPTION FOR " } else { "" },
                table_name(schema, table),
                grantee_sql(grantee),
                if *cascade { " CASCADE" } else { "" }
            )))
        }
        PrivilegeChange::GrantSchema {
            schema,
            privileges,
            grantee,
            grant_option,
        } => {
            let list = require_privileges(privileges, SchemaPrivilege::sql)?;
            Ok(statement(format!(
                "GRANT {list} ON SCHEMA {} TO {}{}",
                quote_ident(schema),
                grantee_sql(grantee),
                if *grant_option { " WITH GRANT OPTION" } else { "" }
            )))
        }
        PrivilegeChange::RevokeSchema {
            schema,
            privileges,
            grantee,
            grant_option_only,
            cascade,
        } => {
            let list = require_privileges(privileges, SchemaPrivilege::sql)?;
            Ok(statement(format!(
                "REVOKE {}{list} ON SCHEMA {} FROM {}{}",
                if *grant_option_only { "GRANT OPTION FOR " } else { "" },
                quote_ident(schema),
                grantee_sql(grantee),
                if *cascade { " CASCADE" } else { "" }
            )))
        }
    }
}

fn require_privileges<T: Copy>(privileges: &[T], sql: impl Fn(T) -> &'static str) -> Result<String> {
    if privileges.is_empty() {
        return Err(Error::Config(
            "hace falta elegir al menos un privilegio".to_owned(),
        ));
    }
    Ok(privileges.iter().copied().map(sql).collect::<Vec<_>>().join(", "))
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[PrivilegeChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Un privilegio ya otorgado, tal como sale de `aclexplode`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegeGrant {
    /// Nombre del rol, o `"PUBLIC"`.
    pub grantee: String,
    /// Tal como lo devuelve `aclexplode`: `"SELECT"`, `"INSERT"`, ...
    pub privilege: String,
    /// Si ese `grantee` puede volver a otorgarlo (`WITH GRANT OPTION`).
    pub grantable: bool,
}

/// Los privilegios que ya tiene una tabla. Una tabla recién creada tiene `relacl` en `NULL` —nadie
/// le tocó los privilegios todavía, rige el default implícito de "el dueño puede todo"— y sin el
/// `coalesce` con `acldefault` esas filas no aparecerían nunca.
pub async fn table_privileges(handle: &ServerHandle, database: &str, oid: u32) -> Result<Vec<PrivilegeGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT CASE WHEN g.grantee = 0 THEN 'PUBLIC' ELSE pg_catalog.pg_get_userbyid(g.grantee)::text END,
                    g.privilege_type,
                    g.is_grantable
               FROM pg_catalog.pg_class c
              CROSS JOIN LATERAL pg_catalog.aclexplode(
                        coalesce(c.relacl, pg_catalog.acldefault('r', c.relowner))) g
              WHERE c.oid = $1
              ORDER BY 1, 2",
            &[&oid],
        )
        .await?;
    Ok(rows_to_grants(rows))
}

/// Los privilegios que ya tiene un esquema. Mismo razonamiento del `coalesce` que
/// [`table_privileges`], con `acldefault('n', ...)`: `'n'` de "namespace", que es como Postgres
/// llama internamente a un esquema.
pub async fn schema_privileges(handle: &ServerHandle, database: &str, oid: u32) -> Result<Vec<PrivilegeGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT CASE WHEN g.grantee = 0 THEN 'PUBLIC' ELSE pg_catalog.pg_get_userbyid(g.grantee)::text END,
                    g.privilege_type,
                    g.is_grantable
               FROM pg_catalog.pg_namespace n
              CROSS JOIN LATERAL pg_catalog.aclexplode(
                        coalesce(n.nspacl, pg_catalog.acldefault('n', n.nspowner))) g
              WHERE n.oid = $1
              ORDER BY 1, 2",
            &[&oid],
        )
        .await?;
    Ok(rows_to_grants(rows))
}

fn rows_to_grants(rows: Vec<tokio_postgres::Row>) -> Vec<PrivilegeGrant> {
    rows.into_iter()
        .map(|row| PrivilegeGrant {
            grantee: row.get(0),
            privilege: row.get(1),
            grantable: row.get(2),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: PrivilegeChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn otorga_privilegios_de_tabla() {
        let statement = one_statement(PrivilegeChange::GrantTable {
            schema: "public".into(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select, TablePrivilege::Insert],
            grantee: "ana".into(),
            grant_option: false,
        });
        assert_eq!(
            statement.sql,
            "GRANT SELECT, INSERT ON public.clientes TO ana"
        );
    }

    #[test]
    fn otorga_con_grant_option() {
        let statement = one_statement(PrivilegeChange::GrantTable {
            schema: "public".into(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select],
            grantee: "ana".into(),
            grant_option: true,
        });
        assert_eq!(
            statement.sql,
            "GRANT SELECT ON public.clientes TO ana WITH GRANT OPTION"
        );
    }

    #[test]
    fn revoca_privilegios_de_tabla_con_cascade() {
        let statement = one_statement(PrivilegeChange::RevokeTable {
            schema: "public".into(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Insert],
            grantee: "ana".into(),
            grant_option_only: false,
            cascade: true,
        });
        assert_eq!(
            statement.sql,
            "REVOKE INSERT ON public.clientes FROM ana CASCADE"
        );
    }

    #[test]
    fn revoca_solo_el_grant_option() {
        let statement = one_statement(PrivilegeChange::RevokeTable {
            schema: "public".into(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select],
            grantee: "ana".into(),
            grant_option_only: true,
            cascade: false,
        });
        assert_eq!(
            statement.sql,
            "REVOKE GRANT OPTION FOR SELECT ON public.clientes FROM ana"
        );
    }

    #[test]
    fn public_no_se_cita_como_identificador() {
        let statement = one_statement(PrivilegeChange::GrantTable {
            schema: "public".into(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select],
            grantee: "PUBLIC".into(),
            grant_option: false,
        });
        assert_eq!(statement.sql, "GRANT SELECT ON public.clientes TO PUBLIC");

        // Sin distinguir mayúsculas: "public" también cuenta.
        let statement = one_statement(PrivilegeChange::GrantTable {
            schema: "public".into(),
            table: "clientes".into(),
            privileges: vec![TablePrivilege::Select],
            grantee: "public".into(),
            grant_option: false,
        });
        assert_eq!(statement.sql, "GRANT SELECT ON public.clientes TO PUBLIC");
    }

    #[test]
    fn otorga_y_revoca_privilegios_de_esquema() {
        let statement = one_statement(PrivilegeChange::GrantSchema {
            schema: "app".into(),
            privileges: vec![SchemaPrivilege::Usage, SchemaPrivilege::Create],
            grantee: "ana".into(),
            grant_option: false,
        });
        assert_eq!(statement.sql, "GRANT USAGE, CREATE ON SCHEMA app TO ana");

        let statement = one_statement(PrivilegeChange::RevokeSchema {
            schema: "app".into(),
            privileges: vec![SchemaPrivilege::Create],
            grantee: "ana".into(),
            grant_option_only: false,
            cascade: false,
        });
        assert_eq!(statement.sql, "REVOKE CREATE ON SCHEMA app FROM ana");
    }

    #[test]
    fn una_lista_vacia_no_se_genera() {
        assert!(statements(&[PrivilegeChange::GrantTable {
            schema: "public".into(),
            table: "clientes".into(),
            privileges: vec![],
            grantee: "ana".into(),
            grant_option: false,
        }])
        .is_err());
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = one_statement(PrivilegeChange::GrantTable {
            schema: "mi esquema".into(),
            table: "Clientes".into(),
            privileges: vec![TablePrivilege::Select],
            grantee: "Ana Gómez".into(),
            grant_option: false,
        });
        assert_eq!(
            statement.sql,
            "GRANT SELECT ON \"mi esquema\".\"Clientes\" TO \"Ana Gómez\""
        );
    }
}
