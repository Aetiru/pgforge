//! Otorgar y revocar privilegios.
//!
//! Postgres tiene un vocabulario de privilegios distinto por tipo de objeto — una tabla admite
//! `SELECT`/`INSERT`/..., un esquema solo `USAGE`/`CREATE`, una función solo `EXECUTE` — así que acá
//! no hay un enum genérico de "privilegio", sino uno por tipo de objeto. `GRANT ALL PRIVILEGES` no
//! es un caso aparte: es exactamente lo mismo que listar todos los privilegios de ese tipo, así que
//! un checkbox "Todos" en la interfaz alcanza con marcarlos todos, sin que el núcleo necesite saber
//! que eso es "todos".
//!
//! Lo que cambia entre un tipo de objeto y otro es el vocabulario y cómo se nombra el objeto; el
//! resto de la sentencia —a quién, con `GRANT OPTION`, con `CASCADE`— es siempre igual. Por eso
//! [`Grantable`] junta las dos cosas que varían y [`PrivilegeChange`] tiene solo cuatro variantes:
//! sumar un tipo de objeto es una variante de `Grantable`, no dos de `PrivilegeChange`.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident};

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

/// Los únicos privilegios que se pueden dar sobre una columna suelta. `DELETE` o `TRUNCATE` no
/// están: borrar una fila no es algo que se pueda hacer "solo en una columna".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColumnPrivilege {
    Select,
    Insert,
    Update,
    References,
}

impl ColumnPrivilege {
    fn sql(self) -> &'static str {
        match self {
            ColumnPrivilege::Select => "SELECT",
            ColumnPrivilege::Insert => "INSERT",
            ColumnPrivilege::Update => "UPDATE",
            ColumnPrivilege::References => "REFERENCES",
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

/// `USAGE` deja usar `nextval`/`currval`; `SELECT` deja leer el valor actual sin avanzarlo, y
/// `UPDATE` deja `setval`. Son tres cosas distintas y por eso no alcanza con el vocabulario de
/// tabla, aunque una secuencia viva en `pg_class` igual que ella.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SequencePrivilege {
    Usage,
    Select,
    Update,
}

impl SequencePrivilege {
    fn sql(self) -> &'static str {
        match self {
            SequencePrivilege::Usage => "USAGE",
            SequencePrivilege::Select => "SELECT",
            SequencePrivilege::Update => "UPDATE",
        }
    }
}

/// Una función o un procedimiento solo admiten `EXECUTE`. El enum de un solo caso existe igual para
/// que el tipo diga sobre qué se está hablando y para que la interfaz no tenga que inventar la
/// palabra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionPrivilege {
    Execute,
}

impl FunctionPrivilege {
    fn sql(self) -> &'static str {
        "EXECUTE"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DatabasePrivilege {
    Connect,
    Create,
    Temporary,
}

impl DatabasePrivilege {
    fn sql(self) -> &'static str {
        match self {
            DatabasePrivilege::Connect => "CONNECT",
            DatabasePrivilege::Create => "CREATE",
            DatabasePrivilege::Temporary => "TEMPORARY",
        }
    }
}

/// `USAGE` es el único privilegio de un tipo o un dominio. Solo aparece en los privilegios por
/// omisión, que es donde tiene sentido darlo antes de que el tipo exista.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TypePrivilege {
    Usage,
}

impl TypePrivilege {
    fn sql(self) -> &'static str {
        "USAGE"
    }
}

/// Sobre qué se otorga o se revoca: el objeto y, atado a él, su vocabulario de privilegios. Que
/// vayan juntos es a propósito — así no se puede pedir `TRUNCATE` sobre un esquema.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "on", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Grantable {
    Table {
        schema: String,
        table: String,
        privileges: Vec<TablePrivilege>,
    },
    /// Privilegios acotados a unas columnas. La lista de columnas se repite en cada privilegio,
    /// que es como lo escribe Postgres: `GRANT SELECT (a, b), UPDATE (a, b) ON t TO r`.
    Columns {
        schema: String,
        table: String,
        columns: Vec<String>,
        privileges: Vec<ColumnPrivilege>,
    },
    Schema {
        schema: String,
        privileges: Vec<SchemaPrivilege>,
    },
    Sequence {
        schema: String,
        sequence: String,
        privileges: Vec<SequencePrivilege>,
    },
    /// `args` sale de [`super::function::identity_args`] y va crudo: es texto ya formateado por el
    /// servidor, igual que el nombre de tipo de una columna.
    Function {
        schema: String,
        name: String,
        args: String,
        /// Desde PG 11 `ON FUNCTION` no alcanza a los procedimientos: hay que decir `ON PROCEDURE`.
        procedure: bool,
        privileges: Vec<FunctionPrivilege>,
    },
    Database {
        database: String,
        privileges: Vec<DatabasePrivilege>,
    },
}

impl Grantable {
    /// El fragmento que va después de `ON`.
    fn object(&self) -> String {
        match self {
            Grantable::Table { schema, table, .. } => qualified(schema, table),
            Grantable::Columns { schema, table, .. } => qualified(schema, table),
            Grantable::Schema { schema, .. } => format!("SCHEMA {}", quote_ident(schema)),
            Grantable::Sequence {
                schema, sequence, ..
            } => format!("SEQUENCE {}", qualified(schema, sequence)),
            Grantable::Function {
                schema,
                name,
                args,
                procedure,
                ..
            } => format!(
                "{} {}({args})",
                super::function::keyword(*procedure),
                qualified(schema, name)
            ),
            Grantable::Database { database, .. } => {
                format!("DATABASE {}", quote_ident(database))
            }
        }
    }

    /// La lista que va entre `GRANT`/`REVOKE` y `ON`.
    fn privileges(&self) -> Result<String> {
        match self {
            Grantable::Table { privileges, .. } => list(privileges, TablePrivilege::sql),
            Grantable::Columns {
                columns,
                privileges,
                ..
            } => {
                if columns.is_empty() {
                    return Err(Error::Config(
                        "hace falta elegir al menos una columna".to_owned(),
                    ));
                }
                let names = columns
                    .iter()
                    .map(|column| quote_ident(column))
                    .collect::<Vec<_>>()
                    .join(", ");
                require_some(privileges)?;
                // La lista de columnas cuelga de cada privilegio, no de la sentencia: es la única
                // forma que acepta Postgres.
                Ok(privileges
                    .iter()
                    .map(|privilege| format!("{} ({names})", privilege.sql()))
                    .collect::<Vec<_>>()
                    .join(", "))
            }
            Grantable::Schema { privileges, .. } => list(privileges, SchemaPrivilege::sql),
            Grantable::Sequence { privileges, .. } => list(privileges, SequencePrivilege::sql),
            Grantable::Function { privileges, .. } => list(privileges, FunctionPrivilege::sql),
            Grantable::Database { privileges, .. } => list(privileges, DatabasePrivilege::sql),
        }
    }
}

/// Sobre qué actúan los privilegios por omisión. El tipo de objeto y su vocabulario van juntos por
/// la misma razón que en [`Grantable`].
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "on", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DefaultPrivileges {
    Tables { privileges: Vec<TablePrivilege> },
    Sequences { privileges: Vec<SequencePrivilege> },
    Functions { privileges: Vec<FunctionPrivilege> },
    Types { privileges: Vec<TypePrivilege> },
}

impl DefaultPrivileges {
    /// La palabra que va después de `ON`, siempre en plural: no se habla de un objeto que exista,
    /// sino de los que se creen de acá en adelante.
    fn objects(&self) -> &'static str {
        match self {
            DefaultPrivileges::Tables { .. } => "TABLES",
            DefaultPrivileges::Sequences { .. } => "SEQUENCES",
            DefaultPrivileges::Functions { .. } => "FUNCTIONS",
            DefaultPrivileges::Types { .. } => "TYPES",
        }
    }

    fn privileges(&self) -> Result<String> {
        match self {
            DefaultPrivileges::Tables { privileges } => list(privileges, TablePrivilege::sql),
            DefaultPrivileges::Sequences { privileges } => list(privileges, SequencePrivilege::sql),
            DefaultPrivileges::Functions { privileges } => list(privileges, FunctionPrivilege::sql),
            DefaultPrivileges::Types { privileges } => list(privileges, TypePrivilege::sql),
        }
    }
}

/// A quién le corresponden los privilegios por omisión que se están definiendo.
///
/// Los dos campos acotan *cuándo* se aplican, no a quién se le otorga: solo valen para lo que cree
/// `role` (por omisión, quien ejecuta la sentencia) dentro de `schema` (por omisión, cualquiera).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultScope {
    pub role: Option<String>,
    pub schema: Option<String>,
}

impl DefaultScope {
    fn sql(&self) -> String {
        let mut out = String::from("ALTER DEFAULT PRIVILEGES");
        if let Some(role) = &self.role {
            out.push_str(&format!(" FOR ROLE {}", super::role_name(role)));
        }
        if let Some(schema) = &self.schema {
            out.push_str(&format!(" IN SCHEMA {}", quote_ident(schema)));
        }
        out
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PrivilegeChange {
    Grant {
        target: Grantable,
        grantee: String,
        grant_option: bool,
    },
    Revoke {
        target: Grantable,
        grantee: String,
        /// `REVOKE GRANT OPTION FOR ...`: revoca solo el permiso de volver a otorgar, no el
        /// privilegio en sí.
        grant_option_only: bool,
        cascade: bool,
    },
    /// Lo que van a recibir los objetos que todavía no existen. Es la única forma de que un GRANT
    /// sobreviva a la próxima tabla que alguien cree.
    GrantDefault {
        scope: DefaultScope,
        target: DefaultPrivileges,
        grantee: String,
        grant_option: bool,
    },
    RevokeDefault {
        scope: DefaultScope,
        target: DefaultPrivileges,
        grantee: String,
        grant_option_only: bool,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Quien recibe o pierde el privilegio. Las palabras clave (`PUBLIC` y compañía) no se citan; ver
/// [`super::role_name`].
fn grantee_sql(name: &str) -> String {
    super::role_name(name)
}

fn require_some<T>(privileges: &[T]) -> Result<()> {
    if privileges.is_empty() {
        return Err(Error::Config(
            "hace falta elegir al menos un privilegio".to_owned(),
        ));
    }
    Ok(())
}

fn list<T: Copy>(privileges: &[T], sql: impl Fn(T) -> &'static str) -> Result<String> {
    require_some(privileges)?;
    Ok(privileges
        .iter()
        .copied()
        .map(sql)
        .collect::<Vec<_>>()
        .join(", "))
}

fn grant_sql(prefix: &str, privileges: &str, object: &str, grantee: &str, option: bool) -> String {
    format!(
        "{prefix}GRANT {privileges} ON {object} TO {}{}",
        grantee_sql(grantee),
        if option { " WITH GRANT OPTION" } else { "" }
    )
}

fn revoke_sql(
    prefix: &str,
    privileges: &str,
    object: &str,
    grantee: &str,
    option_only: bool,
    cascade: bool,
) -> String {
    format!(
        "{prefix}REVOKE {}{privileges} ON {object} FROM {}{}",
        if option_only { "GRANT OPTION FOR " } else { "" },
        grantee_sql(grantee),
        if cascade { " CASCADE" } else { "" }
    )
}

pub fn statements(changes: &[PrivilegeChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &PrivilegeChange) -> Result<Statement> {
    match change {
        PrivilegeChange::Grant {
            target,
            grantee,
            grant_option,
        } => Ok(statement(grant_sql(
            "",
            &target.privileges()?,
            &target.object(),
            grantee,
            *grant_option,
        ))),
        PrivilegeChange::Revoke {
            target,
            grantee,
            grant_option_only,
            cascade,
        } => Ok(statement(revoke_sql(
            "",
            &target.privileges()?,
            &target.object(),
            grantee,
            *grant_option_only,
            *cascade,
        ))),
        PrivilegeChange::GrantDefault {
            scope,
            target,
            grantee,
            grant_option,
        } => Ok(statement(grant_sql(
            &format!("{} ", scope.sql()),
            &target.privileges()?,
            target.objects(),
            grantee,
            *grant_option,
        ))),
        PrivilegeChange::RevokeDefault {
            scope,
            target,
            grantee,
            grant_option_only,
            cascade,
        } => Ok(statement(revoke_sql(
            &format!("{} ", scope.sql()),
            &target.privileges()?,
            target.objects(),
            grantee,
            *grant_option_only,
            *cascade,
        ))),
    }
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(
    handle: &ServerHandle,
    database: &str,
    changes: &[PrivilegeChange],
) -> Result<()> {
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

/// Lo que devuelve `aclexplode` en las tres consultas de abajo: quién, qué, y si puede reotorgarlo.
const GRANT_COLUMNS: &str = "CASE WHEN g.grantee = 0 THEN 'PUBLIC'
                                  ELSE pg_catalog.pg_get_userbyid(g.grantee)::text END,
                             g.privilege_type,
                             g.is_grantable";

/// Los privilegios de cualquier cosa que viva en `pg_class`: tablas, vistas, vistas
/// materializadas, secuencias y tablas externas.
///
/// Un objeto recién creado tiene el ACL en `NULL` —nadie le tocó los privilegios todavía, rige el
/// default implícito de "el dueño puede todo"— y sin el `coalesce` con `acldefault` esas filas no
/// aparecerían nunca. El tipo que espera `acldefault` no es el `relkind`: para una secuencia es
/// `'s'` minúscula (la mayúscula es un servidor externo), y todo lo demás cuenta como relación.
///
/// El cast a `"char"` es obligatorio: un `CASE` devuelve `text`, y `acldefault(text, oid)` no
/// existe. Con la letra escrita como literal el servidor la infiere sola, pero acá no puede.
pub async fn relation_privileges(
    handle: &ServerHandle,
    database: &str,
    oid: u32,
) -> Result<Vec<PrivilegeGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            &format!(
                "SELECT {GRANT_COLUMNS}
                   FROM pg_catalog.pg_class c
                  CROSS JOIN LATERAL pg_catalog.aclexplode(
                            coalesce(c.relacl, pg_catalog.acldefault(
                                (CASE c.relkind WHEN 'S' THEN 's' ELSE 'r' END)::\"char\",
                                c.relowner))) g
                  WHERE c.oid = $1
                  ORDER BY 1, 2"
            ),
            &[&oid],
        )
        .await?;
    Ok(rows_to_grants(rows))
}

/// Los privilegios que ya tiene un esquema. Mismo razonamiento del `coalesce` que
/// [`relation_privileges`], con `acldefault('n', ...)`: `'n'` de "namespace", que es como Postgres
/// llama internamente a un esquema.
pub async fn schema_privileges(
    handle: &ServerHandle,
    database: &str,
    oid: u32,
) -> Result<Vec<PrivilegeGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            &format!(
                "SELECT {GRANT_COLUMNS}
                   FROM pg_catalog.pg_namespace n
                  CROSS JOIN LATERAL pg_catalog.aclexplode(
                            coalesce(n.nspacl, pg_catalog.acldefault('n', n.nspowner))) g
                  WHERE n.oid = $1
                  ORDER BY 1, 2"
            ),
            &[&oid],
        )
        .await?;
    Ok(rows_to_grants(rows))
}

/// Los privilegios de una función o un procedimiento.
pub async fn function_privileges(
    handle: &ServerHandle,
    database: &str,
    oid: u32,
) -> Result<Vec<PrivilegeGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            &format!(
                "SELECT {GRANT_COLUMNS}
                   FROM pg_catalog.pg_proc p
                  CROSS JOIN LATERAL pg_catalog.aclexplode(
                            coalesce(p.proacl, pg_catalog.acldefault('f', p.proowner))) g
                  WHERE p.oid = $1
                  ORDER BY 1, 2"
            ),
            &[&oid],
        )
        .await?;
    Ok(rows_to_grants(rows))
}

/// Los privilegios de una base. `pg_database` es un catálogo compartido por todo el clúster, así
/// que la base sobre la que se consulta da igual.
pub async fn database_privileges(
    handle: &ServerHandle,
    database: &str,
    name: &str,
) -> Result<Vec<PrivilegeGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            &format!(
                "SELECT {GRANT_COLUMNS}
                   FROM pg_catalog.pg_database d
                  CROSS JOIN LATERAL pg_catalog.aclexplode(
                            coalesce(d.datacl, pg_catalog.acldefault('d', d.datdba))) g
                  WHERE d.datname = $1
                  ORDER BY 1, 2"
            ),
            &[&name],
        )
        .await?;
    Ok(rows_to_grants(rows))
}

/// Un privilegio otorgado sobre una columna suelta.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnGrant {
    pub column: String,
    pub grantee: String,
    pub privilege: String,
    pub grantable: bool,
}

/// Los privilegios por columna de una tabla.
///
/// Acá no hay `coalesce` con `acldefault`: una columna sin ACL propio no tiene privilegios
/// "implícitos" que mostrar, hereda los de la tabla. Listar el default de cada columna llenaría la
/// pantalla de filas que no dicen nada.
pub async fn column_privileges(
    handle: &ServerHandle,
    database: &str,
    oid: u32,
) -> Result<Vec<ColumnGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT a.attname,
                    CASE WHEN g.grantee = 0 THEN 'PUBLIC'
                         ELSE pg_catalog.pg_get_userbyid(g.grantee)::text END,
                    g.privilege_type,
                    g.is_grantable
               FROM pg_catalog.pg_attribute a
              CROSS JOIN LATERAL pg_catalog.aclexplode(a.attacl) g
              WHERE a.attrelid = $1
                AND a.attnum > 0
                AND NOT a.attisdropped
              ORDER BY a.attnum, 2, 3",
            &[&oid],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| ColumnGrant {
            column: row.get(0),
            grantee: row.get(1),
            privilege: row.get(2),
            grantable: row.get(3),
        })
        .collect())
}

/// Un privilegio por omisión ya definido, tal como quedó en `pg_default_acl`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultGrant {
    /// El rol cuyas creaciones futuras dispararán el privilegio.
    pub owner: String,
    /// El esquema donde vale, o `None` si vale en todos.
    pub schema: Option<String>,
    /// `"tables"`, `"sequences"`, `"functions"` o `"types"`, para que la interfaz no tenga que
    /// conocer las letras del catálogo.
    pub objects: String,
    pub grantee: String,
    pub privilege: String,
    pub grantable: bool,
}

/// Los privilegios por omisión definidos en la base.
///
/// `defaclobjtype` usa la `'S'` mayúscula para las secuencias, justo al revés que `acldefault`, que
/// las escribe en minúscula. No hay razón detrás: son dos catálogos distintos que eligieron letras
/// distintas, y confundirlas devuelve una lista vacía sin ningún error.
pub async fn default_privileges(
    handle: &ServerHandle,
    database: &str,
) -> Result<Vec<DefaultGrant>> {
    let client = handle.client(database).await?;
    let rows = client
        .query(
            "SELECT pg_catalog.pg_get_userbyid(d.defaclrole)::text,
                    n.nspname,
                    CASE d.defaclobjtype
                        WHEN 'r' THEN 'tables'
                        WHEN 'S' THEN 'sequences'
                        WHEN 'f' THEN 'functions'
                        WHEN 'T' THEN 'types'
                        ELSE d.defaclobjtype::text
                    END,
                    CASE WHEN g.grantee = 0 THEN 'PUBLIC'
                         ELSE pg_catalog.pg_get_userbyid(g.grantee)::text END,
                    g.privilege_type,
                    g.is_grantable
               FROM pg_catalog.pg_default_acl d
               LEFT JOIN pg_catalog.pg_namespace n ON n.oid = d.defaclnamespace
              CROSS JOIN LATERAL pg_catalog.aclexplode(d.defaclacl) g
              ORDER BY 1, 2, 3, 4, 5",
            &[],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| DefaultGrant {
            owner: row.get(0),
            schema: row.get(1),
            objects: row.get(2),
            grantee: row.get(3),
            privilege: row.get(4),
            grantable: row.get(5),
        })
        .collect())
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

    fn grant(target: Grantable, grantee: &str) -> Statement {
        one_statement(PrivilegeChange::Grant {
            target,
            grantee: grantee.to_owned(),
            grant_option: false,
        })
    }

    fn table(privileges: Vec<TablePrivilege>) -> Grantable {
        Grantable::Table {
            schema: "public".into(),
            table: "clientes".into(),
            privileges,
        }
    }

    #[test]
    fn otorga_privilegios_de_tabla() {
        let statement = grant(
            table(vec![TablePrivilege::Select, TablePrivilege::Insert]),
            "ana",
        );
        assert_eq!(
            statement.sql,
            "GRANT SELECT, INSERT ON public.clientes TO ana"
        );
    }

    #[test]
    fn otorga_con_grant_option() {
        let statement = one_statement(PrivilegeChange::Grant {
            target: table(vec![TablePrivilege::Select]),
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
        let statement = one_statement(PrivilegeChange::Revoke {
            target: table(vec![TablePrivilege::Insert]),
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
        let statement = one_statement(PrivilegeChange::Revoke {
            target: table(vec![TablePrivilege::Select]),
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
        let statement = grant(table(vec![TablePrivilege::Select]), "PUBLIC");
        assert_eq!(statement.sql, "GRANT SELECT ON public.clientes TO PUBLIC");

        // Sin distinguir mayúsculas: "public" también cuenta.
        let statement = grant(table(vec![TablePrivilege::Select]), "public");
        assert_eq!(statement.sql, "GRANT SELECT ON public.clientes TO PUBLIC");
    }

    #[test]
    fn otorga_y_revoca_privilegios_de_esquema() {
        let statement = grant(
            Grantable::Schema {
                schema: "app".into(),
                privileges: vec![SchemaPrivilege::Usage, SchemaPrivilege::Create],
            },
            "ana",
        );
        assert_eq!(statement.sql, "GRANT USAGE, CREATE ON SCHEMA app TO ana");

        let statement = one_statement(PrivilegeChange::Revoke {
            target: Grantable::Schema {
                schema: "app".into(),
                privileges: vec![SchemaPrivilege::Create],
            },
            grantee: "ana".into(),
            grant_option_only: false,
            cascade: false,
        });
        assert_eq!(statement.sql, "REVOKE CREATE ON SCHEMA app FROM ana");
    }

    #[test]
    fn otorga_privilegios_de_secuencia() {
        let statement = grant(
            Grantable::Sequence {
                schema: "app".into(),
                sequence: "clientes_id_seq".into(),
                privileges: vec![SequencePrivilege::Usage, SequencePrivilege::Select],
            },
            "ana",
        );
        assert_eq!(
            statement.sql,
            "GRANT USAGE, SELECT ON SEQUENCE app.clientes_id_seq TO ana"
        );
    }

    /// Los argumentos vienen del servidor ya formateados y no se tocan; lo que sí cambia es la
    /// palabra clave, porque `ON FUNCTION` no alcanza a un procedimiento.
    #[test]
    fn distingue_una_funcion_de_un_procedimiento() {
        let statement = grant(
            Grantable::Function {
                schema: "app".into(),
                name: "saldo".into(),
                args: "integer, text".into(),
                procedure: false,
                privileges: vec![FunctionPrivilege::Execute],
            },
            "ana",
        );
        assert_eq!(
            statement.sql,
            "GRANT EXECUTE ON FUNCTION app.saldo(integer, text) TO ana"
        );

        let statement = grant(
            Grantable::Function {
                schema: "app".into(),
                name: "recalcular".into(),
                args: String::new(),
                procedure: true,
                privileges: vec![FunctionPrivilege::Execute],
            },
            "ana",
        );
        assert_eq!(
            statement.sql,
            "GRANT EXECUTE ON PROCEDURE app.recalcular() TO ana"
        );
    }

    #[test]
    fn otorga_privilegios_de_base() {
        let statement = grant(
            Grantable::Database {
                database: "ventas".into(),
                privileges: vec![DatabasePrivilege::Connect, DatabasePrivilege::Temporary],
            },
            "ana",
        );
        assert_eq!(
            statement.sql,
            "GRANT CONNECT, TEMPORARY ON DATABASE ventas TO ana"
        );
    }

    /// La lista de columnas se repite en cada privilegio: es la única forma que acepta Postgres.
    #[test]
    fn otorga_privilegios_por_columna() {
        let statement = grant(
            Grantable::Columns {
                schema: "public".into(),
                table: "clientes".into(),
                columns: vec!["nombre".into(), "correo".into()],
                privileges: vec![ColumnPrivilege::Select, ColumnPrivilege::Update],
            },
            "ana",
        );
        assert_eq!(
            statement.sql,
            "GRANT SELECT (nombre, correo), UPDATE (nombre, correo) ON public.clientes TO ana"
        );
    }

    #[test]
    fn sin_columnas_no_se_genera_un_privilegio_por_columna() {
        assert!(statements(&[PrivilegeChange::Grant {
            target: Grantable::Columns {
                schema: "public".into(),
                table: "clientes".into(),
                columns: vec![],
                privileges: vec![ColumnPrivilege::Select],
            },
            grantee: "ana".into(),
            grant_option: false,
        }])
        .is_err());
    }

    #[test]
    fn define_privilegios_por_omision() {
        let statement = one_statement(PrivilegeChange::GrantDefault {
            scope: DefaultScope {
                role: Some("ana".into()),
                schema: Some("app".into()),
            },
            target: DefaultPrivileges::Tables {
                privileges: vec![TablePrivilege::Select],
            },
            grantee: "lectores".into(),
            grant_option: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER DEFAULT PRIVILEGES FOR ROLE ana IN SCHEMA app \
             GRANT SELECT ON TABLES TO lectores"
        );
    }

    /// Sin rol ni esquema la sentencia vale para lo que cree el usuario actual, en cualquier
    /// esquema: es el caso más común y no lleva ninguna cláusula.
    #[test]
    fn los_privilegios_por_omision_sin_alcance_no_llevan_clausulas() {
        let statement = one_statement(PrivilegeChange::GrantDefault {
            scope: DefaultScope::default(),
            target: DefaultPrivileges::Sequences {
                privileges: vec![SequencePrivilege::Usage],
            },
            grantee: "PUBLIC".into(),
            grant_option: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER DEFAULT PRIVILEGES GRANT USAGE ON SEQUENCES TO PUBLIC"
        );
    }

    #[test]
    fn revoca_privilegios_por_omision() {
        let statement = one_statement(PrivilegeChange::RevokeDefault {
            scope: DefaultScope {
                role: None,
                schema: Some("app".into()),
            },
            target: DefaultPrivileges::Functions {
                privileges: vec![FunctionPrivilege::Execute],
            },
            grantee: "PUBLIC".into(),
            grant_option_only: false,
            cascade: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER DEFAULT PRIVILEGES IN SCHEMA app REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC"
        );
    }

    #[test]
    fn una_lista_vacia_no_se_genera() {
        assert!(statements(&[PrivilegeChange::Grant {
            target: table(vec![]),
            grantee: "ana".into(),
            grant_option: false,
        }])
        .is_err());
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = grant(
            Grantable::Table {
                schema: "mi esquema".into(),
                table: "Clientes".into(),
                privileges: vec![TablePrivilege::Select],
            },
            "Ana Gómez",
        );
        assert_eq!(
            statement.sql,
            "GRANT SELECT ON \"mi esquema\".\"Clientes\" TO \"Ana Gómez\""
        );
    }
}
