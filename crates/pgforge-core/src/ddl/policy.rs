//! Row-Level Security: el filtro por fila de una tabla y las políticas que lo definen.
//!
//! Son dos cosas distintas, y confundirlas es el malentendido clásico de RLS:
//!
//! 1. **El interruptor de la tabla.** `ALTER TABLE ... ENABLE ROW LEVEL SECURITY` es lo único que
//!    hace que Postgres filtre. Una política sobre una tabla sin el interruptor no hace nada; el
//!    interruptor sin políticas no deja pasar ninguna fila, porque lo que no está permitido está
//!    negado.
//! 2. **Las políticas.** Cada una permite algo, y para un mismo comando se combinan con OR. Las
//!    `RESTRICTIVE` funcionan al revés: se combinan con AND y sirven para recortar lo que las
//!    permisivas dejaron pasar, así que una tabla que solo tiene políticas restrictivas tampoco
//!    muestra nada.
//!
//! El dueño de la tabla se saltea el filtro salvo que se pida `FORCE ROW LEVEL SECURITY`, y los
//! roles con `BYPASSRLS` se lo saltean siempre. Eso explica el «a mí me funciona» de quien prueba
//! sus políticas conectado como dueño de la tabla.
//!
//! `USING` y `WITH CHECK` son SQL crudo: misma frontera de confianza que el `WHEN` de un trigger o
//! el `CHECK` de una constraint.
//!
//! Editar una política es borrarla y crearla de nuevo, igual que un trigger. `ALTER POLICY` existe
//! pero solo alcanza a los roles y a las dos expresiones: ni el comando (`FOR`) ni el carácter
//! permisivo o restrictivo (`AS`) se pueden cambiar, así que un editor que ofrezca cambiar todo
//! necesita el camino de borrar y crear igual. Tener uno solo, dentro de una transacción, es más
//! simple que tener dos y decidir cuál usar.
//!
//! Acá no hace falta preguntar por la versión del servidor: `pg_policy` existe desde PostgreSQL 9.5
//! y `polpermissive` desde la 10, las dos muy por debajo del piso soportado.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident, role_name};

/// El comando al que se aplica la política.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    All,
    Select,
    Insert,
    Update,
    Delete,
}

impl Command {
    fn sql(self) -> &'static str {
        match self {
            Command::All => "ALL",
            Command::Select => "SELECT",
            Command::Insert => "INSERT",
            Command::Update => "UPDATE",
            Command::Delete => "DELETE",
        }
    }

    /// La letra de `pg_policy.polcmd`; `'*'` es «todos los comandos».
    fn from_catalog(code: &str) -> Command {
        match code {
            "r" => Command::Select,
            "a" => Command::Insert,
            "w" => Command::Update,
            "d" => Command::Delete,
            _ => Command::All,
        }
    }

    /// `USING` filtra las filas que ya existen, y un `INSERT` no tiene ninguna: Postgres rechaza la
    /// combinación en vez de ignorarla.
    fn accepts_using(self) -> bool {
        !matches!(self, Command::Insert)
    }

    /// `WITH CHECK` verifica la fila que se va a escribir, y ni `SELECT` ni `DELETE` escriben una.
    fn accepts_check(self) -> bool {
        !matches!(self, Command::Select | Command::Delete)
    }
}

/// Si la política suma permisos o los recorta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyKind {
    Permissive,
    Restrictive,
}

/// Lo que define una política nueva.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyDef {
    pub command: Command,
    pub kind: PolicyKind,
    /// Vacío significa `PUBLIC`, que es el default de Postgres.
    pub roles: Vec<String>,
    /// Expresión SQL cruda que decide qué filas se ven.
    pub using: Option<String>,
    /// Expresión SQL cruda que decide qué filas se pueden escribir.
    pub check: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PolicyChange {
    CreatePolicy {
        schema: String,
        table: String,
        name: String,
        definition: PolicyDef,
    },
    DropPolicy {
        schema: String,
        table: String,
        name: String,
    },
    /// El interruptor: sin esto las políticas no se aplican.
    SetRowSecurity {
        schema: String,
        table: String,
        enabled: bool,
    },
    /// Que el filtro alcance también al dueño de la tabla.
    SetForceRowSecurity {
        schema: String,
        table: String,
        forced: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

pub fn statements(changes: &[PolicyChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &PolicyChange) -> Result<Statement> {
    match change {
        PolicyChange::CreatePolicy {
            schema,
            table,
            name,
            definition,
        } => create_policy(schema, table, name, definition),
        PolicyChange::DropPolicy {
            schema,
            table,
            name,
        } => Ok(statement(format!(
            "DROP POLICY {} ON {}",
            quote_ident(name),
            qualified(schema, table)
        ))),
        PolicyChange::SetRowSecurity {
            schema,
            table,
            enabled,
        } => Ok(statement(format!(
            "ALTER TABLE {} {} ROW LEVEL SECURITY",
            qualified(schema, table),
            if *enabled { "ENABLE" } else { "DISABLE" }
        ))),
        PolicyChange::SetForceRowSecurity {
            schema,
            table,
            forced,
        } => Ok(statement(format!(
            "ALTER TABLE {} {} ROW LEVEL SECURITY",
            qualified(schema, table),
            if *forced { "FORCE" } else { "NO FORCE" }
        ))),
    }
}

fn create_policy(schema: &str, table: &str, name: &str, def: &PolicyDef) -> Result<Statement> {
    if name.trim().is_empty() {
        return Err(Error::Config("una política necesita un nombre".to_owned()));
    }

    let using = expression(&def.using);
    let check = expression(&def.check);

    // Se rechaza acá, y no dejando que falle el servidor, porque el mensaje que llega de vuelta
    // («only WITH CHECK expression allowed for INSERT») no dice qué hacer con el formulario.
    if using.is_some() && !def.command.accepts_using() {
        return Err(Error::Config(
            "una política de INSERT no admite USING: no hay filas previas que filtrar, \
             solo se puede verificar la fila nueva con WITH CHECK"
                .to_owned(),
        ));
    }
    if check.is_some() && !def.command.accepts_check() {
        return Err(Error::Config(
            "una política de SELECT o DELETE no admite WITH CHECK: no escribe ninguna fila que \
             verificar"
                .to_owned(),
        ));
    }

    let mut sql = format!(
        "CREATE POLICY {}\n    ON {}",
        quote_ident(name),
        qualified(schema, table)
    );

    // `PERMISSIVE` es el default de Postgres y escribirlo solo agrega ruido; `RESTRICTIVE` cambia
    // cómo se combina con las demás, así que ahí sí conviene verlo en la vista previa.
    if def.kind == PolicyKind::Restrictive {
        sql.push_str("\n    AS RESTRICTIVE");
    }

    sql.push_str(&format!("\n    FOR {}", def.command.sql()));
    sql.push_str(&format!("\n    TO {}", roles_sql(&def.roles)));

    if let Some(using) = using {
        sql.push_str(&format!("\n    USING ({using})"));
    }
    if let Some(check) = check {
        sql.push_str(&format!("\n    WITH CHECK ({check})"));
    }

    Ok(statement(sql))
}

/// Una expresión en blanco es una expresión ausente: el formulario deja el campo vacío, no manda
/// `null`.
fn expression(raw: &Option<String>) -> Option<&str> {
    raw.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn roles_sql(roles: &[String]) -> String {
    let named: Vec<String> = roles
        .iter()
        .map(|role| role.trim())
        .filter(|role| !role.is_empty())
        .map(role_name)
        .collect();

    if named.is_empty() {
        "PUBLIC".to_owned()
    } else {
        named.join(", ")
    }
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[PolicyChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Una política tal como ya existe.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyInfo {
    pub oid: u32,
    pub name: String,
    pub command: Command,
    pub kind: PolicyKind,
    /// Vacío significa `PUBLIC`.
    pub roles: Vec<String>,
    pub using: Option<String>,
    pub check: Option<String>,
}

/// El estado de RLS de una tabla: el interruptor y las políticas, juntos.
///
/// Van en la misma respuesta porque separados no se entienden: una lista de tres políticas sobre
/// una tabla con el filtro apagado no está haciendo nada, y mostrarla sin decirlo haría creer lo
/// contrario.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSecurity {
    pub enabled: bool,
    pub forced: bool,
    pub policies: Vec<PolicyInfo>,
}

const POLICIES_SQL: &str = "
    SELECT p.oid,
           p.polname::text,
           p.polcmd::text,
           p.polpermissive,
           pg_catalog.pg_get_expr(p.polqual, p.polrelid),
           pg_catalog.pg_get_expr(p.polwithcheck, p.polrelid),
           (SELECT coalesce(
                       array_agg(pg_catalog.pg_get_userbyid(r)::text ORDER BY
                                 pg_catalog.pg_get_userbyid(r)::text),
                       ARRAY[]::text[])
              FROM unnest(p.polroles) AS r
             WHERE r <> 0)
      FROM pg_catalog.pg_policy p
     WHERE p.polrelid = $1
     ORDER BY p.polname
";

/// El estado de RLS de una tabla.
pub async fn table_security(
    handle: &ServerHandle,
    database: &str,
    oid: u32,
) -> Result<TableSecurity> {
    let client = handle.client(database).await?;

    let switches = client
        .query_one(
            "SELECT c.relrowsecurity, c.relforcerowsecurity
               FROM pg_catalog.pg_class c
              WHERE c.oid = $1",
            &[&oid],
        )
        .await?;

    let rows = client.query(POLICIES_SQL, &[&oid]).await?;

    Ok(TableSecurity {
        enabled: switches.get(0),
        forced: switches.get(1),
        policies: rows.into_iter().map(row_to_policy).collect(),
    })
}

fn row_to_policy(row: tokio_postgres::Row) -> PolicyInfo {
    let permissive: bool = row.get(3);
    PolicyInfo {
        oid: row.get(0),
        name: row.get(1),
        command: Command::from_catalog(row.get(2)),
        kind: if permissive {
            PolicyKind::Permissive
        } else {
            PolicyKind::Restrictive
        },
        // `polroles` guarda un 0 para PUBLIC, y `pg_get_userbyid(0)` no devuelve ningún rol: la
        // consulta lo descarta, así que PUBLIC llega como lista vacía.
        roles: row.get(6),
        using: row.get(4),
        check: row.get(5),
    }
}

/// El `CREATE POLICY` de una política que ya existe, para el panel de DDL.
///
/// No hay ninguna función del servidor que lo arme —`pg_get_policydef` no existe—, así que se
/// reconstruye con el mismo generador que usa la vista previa: lo que se muestra acá es exactamente
/// lo que pgforge ejecutaría para volver a crearla.
pub async fn describe(handle: &ServerHandle, database: &str, oid: u32) -> Result<String> {
    let client = handle.client(database).await?;

    let row = client
        .query_opt(
            "SELECT p.polrelid, n.nspname::text, c.relname::text
               FROM pg_catalog.pg_policy p
               JOIN pg_catalog.pg_class c ON c.oid = p.polrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE p.oid = $1",
            &[&oid],
        )
        .await?
        .ok_or_else(|| Error::Config("la política ya no existe".to_owned()))?;

    let relid: u32 = row.get(0);
    let schema: String = row.get(1);
    let table: String = row.get(2);

    let security = table_security(handle, database, relid).await?;
    let policy = security
        .policies
        .into_iter()
        .find(|policy| policy.oid == oid)
        .ok_or_else(|| Error::Config("la política ya no existe".to_owned()))?;

    let statement = create_policy(
        &schema,
        &table,
        &policy.name,
        &PolicyDef {
            command: policy.command,
            kind: policy.kind,
            roles: policy.roles,
            using: policy.using,
            check: policy.check,
        },
    )?;

    Ok(format!("{};", statement.sql))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> PolicyDef {
        PolicyDef {
            command: Command::All,
            kind: PolicyKind::Permissive,
            roles: vec![],
            using: None,
            check: None,
        }
    }

    fn one_statement(change: PolicyChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    fn create(definition: PolicyDef) -> String {
        one_statement(PolicyChange::CreatePolicy {
            schema: "public".into(),
            table: "clientes".into(),
            name: "solo_los_propios".into(),
            definition,
        })
        .sql
    }

    #[test]
    fn crea_una_politica_minima() {
        assert_eq!(
            create(def()),
            "CREATE POLICY solo_los_propios\n    \
             ON public.clientes\n    \
             FOR ALL\n    \
             TO PUBLIC"
        );
    }

    #[test]
    fn sin_roles_va_a_public() {
        let mut d = def();
        d.roles = vec!["  ".into()];
        assert!(create(d).contains("\n    TO PUBLIC"));
    }

    #[test]
    fn lista_los_roles_elegidos() {
        let mut d = def();
        d.roles = vec!["ana".into(), "beto".into()];
        assert!(create(d).contains("\n    TO ana, beto"));
    }

    #[test]
    fn public_y_los_pseudoroles_no_se_citan() {
        for palabra in ["PUBLIC", "public", "current_user", "SESSION_USER"] {
            let mut d = def();
            d.roles = vec![palabra.into()];
            let sql = create(d);
            assert!(
                sql.contains(&format!("\n    TO {}", palabra.to_uppercase())),
                "{palabra}: {sql}"
            );
        }
    }

    #[test]
    fn permissive_no_se_escribe_y_restrictive_si() {
        assert!(!create(def()).contains("AS "));

        let mut d = def();
        d.kind = PolicyKind::Restrictive;
        assert!(create(d).contains("\n    AS RESTRICTIVE\n"));
    }

    #[test]
    fn cada_comando_se_traduce() {
        for (command, palabra) in [
            (Command::All, "ALL"),
            (Command::Select, "SELECT"),
            (Command::Insert, "INSERT"),
            (Command::Update, "UPDATE"),
            (Command::Delete, "DELETE"),
        ] {
            let mut d = def();
            d.command = command;
            // `USING` no vale para INSERT, así que se prueba el comando pelado.
            let sql = create(d);
            assert!(
                sql.contains(&format!("\n    FOR {palabra}\n")),
                "{palabra}: {sql}"
            );
        }
    }

    #[test]
    fn agrega_las_dos_expresiones() {
        let mut d = def();
        d.using = Some("dueno = current_user".into());
        d.check = Some("dueno = current_user".into());
        let sql = create(d);
        assert!(sql.contains("\n    USING (dueno = current_user)"), "{sql}");
        assert!(
            sql.contains("\n    WITH CHECK (dueno = current_user)"),
            "{sql}"
        );
    }

    #[test]
    fn una_expresion_en_blanco_es_una_expresion_ausente() {
        let mut d = def();
        d.using = Some("   ".into());
        let sql = create(d);
        assert!(!sql.contains("USING"), "{sql}");
    }

    #[test]
    fn insert_no_admite_using() {
        let mut d = def();
        d.command = Command::Insert;
        d.using = Some("true".into());
        assert!(statements(&[PolicyChange::CreatePolicy {
            schema: "public".into(),
            table: "clientes".into(),
            name: "p".into(),
            definition: d,
        }])
        .is_err());
    }

    #[test]
    fn select_y_delete_no_admiten_with_check() {
        for command in [Command::Select, Command::Delete] {
            let mut d = def();
            d.command = command;
            d.check = Some("true".into());
            assert!(
                statements(&[PolicyChange::CreatePolicy {
                    schema: "public".into(),
                    table: "clientes".into(),
                    name: "p".into(),
                    definition: d,
                }])
                .is_err(),
                "{command:?} tendría que haber sido rechazado"
            );
        }
    }

    #[test]
    fn insert_si_admite_with_check() {
        let mut d = def();
        d.command = Command::Insert;
        d.check = Some("true".into());
        assert!(create(d).contains("WITH CHECK (true)"));
    }

    #[test]
    fn una_politica_sin_nombre_no_se_genera() {
        assert!(statements(&[PolicyChange::CreatePolicy {
            schema: "public".into(),
            table: "clientes".into(),
            name: "  ".into(),
            definition: def(),
        }])
        .is_err());
    }

    #[test]
    fn borra_una_politica() {
        let statement = one_statement(PolicyChange::DropPolicy {
            schema: "public".into(),
            table: "clientes".into(),
            name: "solo_los_propios".into(),
        });
        assert_eq!(
            statement.sql,
            "DROP POLICY solo_los_propios ON public.clientes"
        );
    }

    #[test]
    fn prende_y_apaga_el_filtro() {
        let sql = |enabled| {
            one_statement(PolicyChange::SetRowSecurity {
                schema: "public".into(),
                table: "clientes".into(),
                enabled,
            })
            .sql
        };
        assert_eq!(
            sql(true),
            "ALTER TABLE public.clientes ENABLE ROW LEVEL SECURITY"
        );
        assert_eq!(
            sql(false),
            "ALTER TABLE public.clientes DISABLE ROW LEVEL SECURITY"
        );
    }

    #[test]
    fn fuerza_el_filtro_para_el_dueno() {
        let sql = |forced| {
            one_statement(PolicyChange::SetForceRowSecurity {
                schema: "public".into(),
                table: "clientes".into(),
                forced,
            })
            .sql
        };
        assert_eq!(
            sql(true),
            "ALTER TABLE public.clientes FORCE ROW LEVEL SECURITY"
        );
        assert_eq!(
            sql(false),
            "ALTER TABLE public.clientes NO FORCE ROW LEVEL SECURITY"
        );
    }

    #[test]
    fn traduce_las_letras_del_catalogo() {
        assert_eq!(Command::from_catalog("r"), Command::Select);
        assert_eq!(Command::from_catalog("a"), Command::Insert);
        assert_eq!(Command::from_catalog("w"), Command::Update);
        assert_eq!(Command::from_catalog("d"), Command::Delete);
        assert_eq!(Command::from_catalog("*"), Command::All);
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let mut d = def();
        d.roles = vec!["Ana Gómez".into()];
        let statement = one_statement(PolicyChange::CreatePolicy {
            schema: "mi esquema".into(),
            table: "Clientes".into(),
            name: "Mi Política".into(),
            definition: d,
        });
        assert!(
            statement.sql.contains("CREATE POLICY \"Mi Política\""),
            "{}",
            statement.sql
        );
        assert!(
            statement.sql.contains("ON \"mi esquema\".\"Clientes\""),
            "{}",
            statement.sql
        );
        assert!(
            statement.sql.contains("TO \"Ana Gómez\""),
            "{}",
            statement.sql
        );
    }
}
