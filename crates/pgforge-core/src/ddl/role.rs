//! Crear, cambiar, borrar y agrupar roles.
//!
//! A diferencia de todo lo demás en [`crate::ddl`], un rol no es de una base ni de un esquema: es
//! del clúster entero, visible igual desde cualquier base a la que se esté conectado.
//!
//! `GRANT`/`REVOKE` acá son de membresía (`GRANT rol TO rol`, pertenencia a un grupo), no de
//! privilegios sobre un objeto (`GRANT privilegio ON objeto TO rol`) — Postgres reusa la misma
//! palabra para las dos cosas, pero son sentencias distintas y esto solo cubre la primera.
//!
//! La contraseña y la fecha de `VALID UNTIL` se citan como literales (comillas simples, dobladas si
//! hace falta) y no como identificadores ni como SQL crudo: son valores, no expresiones. Es la
//! única parte de `ddl` que cita un literal en vez de confiar en que el usuario escriba SQL válido,
//! porque acá el campo es un dato de verdad, no una expresión como el `default` de una columna.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::quote_ident;
use super::table::Statement;

/// Los atributos de un rol. `None` en cada campo significa "no tocar": al crear, la UI manda todo
/// en `Some(...)`; al alterar, solo lo que cambió.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAttributes {
    pub superuser: Option<bool>,
    pub createdb: Option<bool>,
    pub createrole: Option<bool>,
    pub inherit: Option<bool>,
    pub login: Option<bool>,
    pub replication: Option<bool>,
    pub bypass_rls: Option<bool>,
    pub connection_limit: Option<i32>,
    /// `None` en edición significa "no tocar la contraseña": Postgres nunca la devuelve, así que no
    /// hay con qué precargar el campo y compararlo.
    pub password: Option<String>,
    /// Texto crudo (`'infinity'` o una fecha), citado como literal.
    pub valid_until: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RoleChange {
    CreateRole {
        name: String,
        attributes: RoleAttributes,
        member_of: Vec<String>,
    },
    AlterRole {
        name: String,
        attributes: RoleAttributes,
    },
    RenameRole {
        name: String,
        new_name: String,
    },
    /// Postgres no tiene `DROP ROLE ... CASCADE`: si el rol es dueño de algún objeto, el error del
    /// servidor lo va a decir, y hay que reasignarlo antes de poder borrarlo — con
    /// [`RoleChange::ReassignOwned`] o [`RoleChange::DropOwned`].
    DropRole {
        name: String,
    },
    /// Le pasa a otro rol todo lo que `from` tiene en la base **conectada**. Las dos sentencias de
    /// abajo son las únicas de este módulo que no alcanzan al clúster entero: hay que repetirlas en
    /// cada base donde el rol tenga algo, y no hay forma de saber desde acá cuáles son.
    ReassignOwned {
        from: String,
        to: String,
    },
    /// Borra lo que el rol tiene en la base conectada, en vez de pasárselo a otro. Es lo que hace
    /// falta para los privilegios que le hayan otorgado: `REASSIGN OWNED` mueve la propiedad pero
    /// deja los `GRANT`, y esos también impiden borrar el rol.
    DropOwned {
        role: String,
        cascade: bool,
    },
    GrantMembership {
        role: String,
        member: String,
        admin_option: bool,
    },
    RevokeMembership {
        role: String,
        member: String,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Comillas simples dobladas: el mismo criterio que ya usa `type_ddl` en `ddl/mod.rs` para las
/// etiquetas de un enum, la única otra parte del core que cita un literal.
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn attributes_clause(attrs: &RoleAttributes) -> String {
    let mut parts = Vec::new();

    let mut flag = |value: Option<bool>, on: &'static str, off: &'static str| {
        if let Some(value) = value {
            parts.push(if value { on } else { off }.to_owned());
        }
    };
    flag(attrs.superuser, "SUPERUSER", "NOSUPERUSER");
    flag(attrs.createdb, "CREATEDB", "NOCREATEDB");
    flag(attrs.createrole, "CREATEROLE", "NOCREATEROLE");
    flag(attrs.inherit, "INHERIT", "NOINHERIT");
    flag(attrs.login, "LOGIN", "NOLOGIN");
    flag(attrs.replication, "REPLICATION", "NOREPLICATION");
    flag(attrs.bypass_rls, "BYPASSRLS", "NOBYPASSRLS");

    if let Some(limit) = attrs.connection_limit {
        parts.push(format!("CONNECTION LIMIT {limit}"));
    }
    if let Some(password) = &attrs.password {
        parts.push(format!("PASSWORD {}", quote_literal(password)));
    }
    if let Some(until) = &attrs.valid_until {
        parts.push(format!("VALID UNTIL {}", quote_literal(until)));
    }

    parts.join(" ")
}

pub fn statements(changes: &[RoleChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &RoleChange) -> Result<Statement> {
    match change {
        RoleChange::CreateRole {
            name,
            attributes,
            member_of,
        } => {
            let mut sql = format!("CREATE ROLE {}", quote_ident(name));
            let clause = attributes_clause(attributes);
            if !clause.is_empty() {
                sql.push_str(&format!(" WITH {clause}"));
            }
            if !member_of.is_empty() {
                let roles = member_of
                    .iter()
                    .map(|r| quote_ident(r))
                    .collect::<Vec<_>>()
                    .join(", ");
                sql.push_str(&format!(" IN ROLE {roles}"));
            }
            Ok(statement(sql))
        }
        RoleChange::AlterRole { name, attributes } => {
            let clause = attributes_clause(attributes);
            if clause.is_empty() {
                return Err(Error::Config(
                    "no hay ningún atributo que cambiar".to_owned(),
                ));
            }
            Ok(statement(format!(
                "ALTER ROLE {} WITH {clause}",
                quote_ident(name)
            )))
        }
        RoleChange::RenameRole { name, new_name } => Ok(statement(format!(
            "ALTER ROLE {} RENAME TO {}",
            quote_ident(name),
            quote_ident(new_name)
        ))),
        RoleChange::DropRole { name } => Ok(statement(format!("DROP ROLE {}", quote_ident(name)))),
        RoleChange::ReassignOwned { from, to } => Ok(statement(format!(
            "REASSIGN OWNED BY {} TO {}",
            quote_ident(from),
            super::role_name(to)
        ))),
        RoleChange::DropOwned { role, cascade } => Ok(statement(format!(
            "DROP OWNED BY {}{}",
            quote_ident(role),
            if *cascade { " CASCADE" } else { "" }
        ))),
        RoleChange::GrantMembership {
            role,
            member,
            admin_option,
        } => Ok(statement(format!(
            "GRANT {} TO {}{}",
            quote_ident(role),
            quote_ident(member),
            if *admin_option {
                " WITH ADMIN OPTION"
            } else {
                ""
            }
        ))),
        RoleChange::RevokeMembership { role, member } => Ok(statement(format!(
            "REVOKE {} FROM {}",
            quote_ident(role),
            quote_ident(member)
        ))),
    }
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[RoleChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Un rol tal como ya existe, para precargar el diálogo de edición.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleInfo {
    pub oid: u32,
    pub name: String,
    pub superuser: bool,
    pub createdb: bool,
    pub createrole: bool,
    pub inherit: bool,
    pub login: bool,
    pub replication: bool,
    pub bypass_rls: bool,
    pub connection_limit: i32,
    pub valid_until: Option<String>,
}

const ROLE_SQL: &str = "
    SELECT r.oid, r.rolname::text, r.rolsuper, r.rolcreatedb, r.rolcreaterole, r.rolinherit,
           r.rolcanlogin, r.rolreplication, r.rolbypassrls, r.rolconnlimit,
           r.rolvaliduntil::text
      FROM pg_catalog.pg_roles r
     WHERE r.oid = $1
";

fn row_to_role(row: &tokio_postgres::Row) -> RoleInfo {
    RoleInfo {
        oid: row.get(0),
        name: row.get(1),
        superuser: row.get(2),
        createdb: row.get(3),
        createrole: row.get(4),
        inherit: row.get(5),
        login: row.get(6),
        replication: row.get(7),
        bypass_rls: row.get(8),
        connection_limit: row.get(9),
        valid_until: row.get(10),
    }
}

/// El rol tal como ya existe, por oid. Lee de `pg_roles`, no de `pg_authid`: esa vista sí requiere
/// ser superusuario para leerla, `pg_roles` la puede leer cualquiera.
pub async fn role(handle: &ServerHandle, database: &str, oid: u32) -> Result<RoleInfo> {
    let client = handle.client(database).await?;
    let row = client
        .query_opt(ROLE_SQL, &[&oid])
        .await?
        .ok_or_else(|| Error::Config(format!("no existe ningún rol con el oid {oid}")))?;
    Ok(row_to_role(&row))
}

/// De qué roles es miembro `name`, para precargar "miembro de" en el diálogo de edición.
pub async fn role_memberships(
    handle: &ServerHandle,
    database: &str,
    name: &str,
) -> Result<Vec<String>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT r2.rolname::text
               FROM pg_catalog.pg_auth_members m
               JOIN pg_catalog.pg_roles r1 ON r1.oid = m.member
               JOIN pg_catalog.pg_roles r2 ON r2.oid = m.roleid
              WHERE r1.rolname = $1
              ORDER BY r2.rolname",
            &[&name],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

/// Reconstruye el `CREATE ROLE` para mostrarlo en el panel de DDL. Sin `PASSWORD`: Postgres nunca
/// la expone, así que no hay forma honesta de incluirla.
pub fn describe(info: &RoleInfo) -> String {
    let attributes = RoleAttributes {
        superuser: Some(info.superuser),
        createdb: Some(info.createdb),
        createrole: Some(info.createrole),
        inherit: Some(info.inherit),
        login: Some(info.login),
        replication: Some(info.replication),
        bypass_rls: Some(info.bypass_rls),
        connection_limit: Some(info.connection_limit),
        password: None,
        valid_until: info.valid_until.clone(),
    };
    let clause = attributes_clause(&attributes);
    let mut sql = format!("CREATE ROLE {}", quote_ident(&info.name));
    if !clause.is_empty() {
        sql.push_str(&format!(" WITH {clause}"));
    }
    sql.push(';');
    sql
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: RoleChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn crea_un_rol_con_atributos() {
        let statement = one_statement(RoleChange::CreateRole {
            name: "lectora".into(),
            attributes: RoleAttributes {
                login: Some(true),
                connection_limit: Some(5),
                ..Default::default()
            },
            member_of: vec![],
        });
        assert_eq!(
            statement.sql,
            "CREATE ROLE lectora WITH LOGIN CONNECTION LIMIT 5"
        );
    }

    #[test]
    fn crea_un_rol_sin_atributos() {
        let statement = one_statement(RoleChange::CreateRole {
            name: "vacio".into(),
            attributes: RoleAttributes::default(),
            member_of: vec![],
        });
        assert_eq!(statement.sql, "CREATE ROLE vacio");
    }

    #[test]
    fn crea_un_rol_miembro_de_otros() {
        let statement = one_statement(RoleChange::CreateRole {
            name: "app".into(),
            attributes: RoleAttributes::default(),
            member_of: vec!["lectores".into(), "escritores".into()],
        });
        assert_eq!(
            statement.sql,
            "CREATE ROLE app IN ROLE lectores, escritores"
        );
    }

    #[test]
    fn altera_solo_lo_que_cambio() {
        let statement = one_statement(RoleChange::AlterRole {
            name: "lectora".into(),
            attributes: RoleAttributes {
                superuser: Some(true),
                ..Default::default()
            },
        });
        assert_eq!(statement.sql, "ALTER ROLE lectora WITH SUPERUSER");
    }

    #[test]
    fn alterar_sin_nada_seteado_no_se_genera() {
        assert!(statements(&[RoleChange::AlterRole {
            name: "lectora".into(),
            attributes: RoleAttributes::default(),
        }])
        .is_err());
    }

    #[test]
    fn renombra_un_rol() {
        let statement = one_statement(RoleChange::RenameRole {
            name: "lectora".into(),
            new_name: "lector".into(),
        });
        assert_eq!(statement.sql, "ALTER ROLE lectora RENAME TO lector");
    }

    #[test]
    fn borra_un_rol() {
        let statement = one_statement(RoleChange::DropRole {
            name: "lectora".into(),
        });
        assert_eq!(statement.sql, "DROP ROLE lectora");
    }

    /// Los dos pasos que hay que dar antes de poder borrar un rol que es dueño de algo.
    #[test]
    fn reasigna_y_suelta_lo_que_el_rol_tiene() {
        let statement = one_statement(RoleChange::ReassignOwned {
            from: "lectora".into(),
            to: "ana".into(),
        });
        assert_eq!(statement.sql, "REASSIGN OWNED BY lectora TO ana");

        let statement = one_statement(RoleChange::DropOwned {
            role: "lectora".into(),
            cascade: true,
        });
        assert_eq!(statement.sql, "DROP OWNED BY lectora CASCADE");
    }

    /// `CURRENT_USER` es una palabra clave y no se cita, igual que en un `GRANT`.
    #[test]
    fn reasigna_al_usuario_actual() {
        let statement = one_statement(RoleChange::ReassignOwned {
            from: "Mi Rol".into(),
            to: "current_user".into(),
        });
        assert_eq!(
            statement.sql,
            "REASSIGN OWNED BY \"Mi Rol\" TO CURRENT_USER"
        );
    }

    #[test]
    fn otorga_y_revoca_membresia() {
        let statement = one_statement(RoleChange::GrantMembership {
            role: "lectores".into(),
            member: "ana".into(),
            admin_option: true,
        });
        assert_eq!(statement.sql, "GRANT lectores TO ana WITH ADMIN OPTION");

        let statement = one_statement(RoleChange::GrantMembership {
            role: "lectores".into(),
            member: "ana".into(),
            admin_option: false,
        });
        assert_eq!(statement.sql, "GRANT lectores TO ana");

        let statement = one_statement(RoleChange::RevokeMembership {
            role: "lectores".into(),
            member: "ana".into(),
        });
        assert_eq!(statement.sql, "REVOKE lectores FROM ana");
    }

    #[test]
    fn cita_la_contrasena_como_literal_con_comillas_dobladas() {
        let statement = one_statement(RoleChange::CreateRole {
            name: "app".into(),
            attributes: RoleAttributes {
                password: Some("no'muy'segura".into()),
                ..Default::default()
            },
            member_of: vec![],
        });
        assert_eq!(
            statement.sql,
            "CREATE ROLE app WITH PASSWORD 'no''muy''segura'"
        );
    }

    #[test]
    fn cita_valid_until_como_literal() {
        let statement = one_statement(RoleChange::CreateRole {
            name: "app".into(),
            attributes: RoleAttributes {
                valid_until: Some("infinity".into()),
                ..Default::default()
            },
            member_of: vec![],
        });
        assert!(
            statement.sql.contains("VALID UNTIL 'infinity'"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn cita_los_nombres_que_lo_necesitan() {
        let statement = one_statement(RoleChange::CreateRole {
            name: "Mi Rol".into(),
            attributes: RoleAttributes::default(),
            member_of: vec![],
        });
        assert_eq!(statement.sql, "CREATE ROLE \"Mi Rol\"");
    }
}
